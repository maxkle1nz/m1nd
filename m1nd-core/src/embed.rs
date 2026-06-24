// === crates/m1nd-core/src/embed.rs ===
//
// OPTIONAL real local semantic embedding for `seek`, behind the `embed` cargo
// feature (OFF by default). When the feature is disabled this file is NOT
// compiled and nothing in the crate changes.
//
// HONESTY: this uses FAST STATIC embeddings (model2vec / potion-base-8M, ~29 MB) —
// a real upgrade over label-only character-trigram TF-IDF, but NOT
// transformer-grade. There is no attention / contextualization: each token maps
// to a fixed vector and the sentence vector is a (zipf/PCA-weighted) pooled mean.
// The ONNX / bge transformer tier is the path for maximal quality. Nothing here
// should be read as implying transformer-grade semantics.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use model2vec_rs::model::StaticModel;

use crate::error::M1ndError;
use crate::error::M1ndResult;

/// Default relative path to the local model directory, resolved against the
/// crate manifest dir at compile time. The blob is gitignored (not committed):
/// fetch it on first use (`minishlab/potion-multilingual-128M` from Hugging Face)
/// or point [`MODEL_DIR_ENV`] at a vendored copy. At runtime we load from disk.
pub const DEFAULT_MODEL_SUBDIR: &str = "assets/potion-base-8M";

/// Canonical Hugging Face repo id for the default model — `model2vec-rs` resolves
/// this and caches it under `~/.cache/huggingface` when no local dir is present
/// (download-on-first-use), keeping the repository free of a ~120 MB blob.
pub const DEFAULT_HF_REPO: &str = "minishlab/potion-base-8M";

/// Environment variable to override the model directory at runtime.
pub const MODEL_DIR_ENV: &str = "M1ND_EMBED_MODEL";

/// A text embedder producing L2-normalized dense vectors.
pub trait Embedder: Send + Sync {
    /// Embed a single text into an L2-normalized vector.
    fn embed(&self, text: &str) -> Box<[f32]>;
    /// The output embedding dimension.
    fn dim(&self) -> usize;
}

/// model2vec-backed static embedder.
///
/// NOTE on dimension: `dim()` reports the model's ACTUAL output dimension, probed
/// once at load time — never a hard-coded value. The default `potion-base-8M` is
/// small and lean (~29 MB); other potion models differ in size and dimension
/// (e.g. `potion-retrieval-32M` is 512-dim, `potion-multilingual-128M` is ~489 MB),
/// and any model2vec directory or HF repo loads unchanged via `M1ND_EMBED_MODEL`.
pub struct Model2VecEmbedder {
    model: Arc<StaticModel>,
    dim: usize,
}

impl Model2VecEmbedder {
    /// Resolve the model directory: `M1ND_EMBED_MODEL` env var if set, else the
    /// vendored `assets/potion-retrieval-32M` directory next to the crate.
    pub fn default_model_dir() -> PathBuf {
        if let Ok(p) = std::env::var(MODEL_DIR_ENV) {
            return PathBuf::from(p);
        }
        Path::new(env!("CARGO_MANIFEST_DIR")).join(DEFAULT_MODEL_SUBDIR)
    }

    /// Load the default model: the resolved local directory if it exists, else
    /// download-on-first-use from Hugging Face ([`DEFAULT_HF_REPO`], cached
    /// locally), so the repository carries no model blob.
    pub fn from_default() -> M1ndResult<Self> {
        let dir = Self::default_model_dir();
        if dir.exists() {
            return Self::from_dir(dir);
        }
        Self::from_hf(DEFAULT_HF_REPO)
    }

    /// Load by Hugging Face repo id. `model2vec-rs` (with the `hf-hub` feature)
    /// downloads and caches the model on first use under `~/.cache/huggingface`.
    pub fn from_hf(repo: &str) -> M1ndResult<Self> {
        let model = StaticModel::from_pretrained(repo, None, None, None).map_err(|e| {
            M1ndError::EmbedError(format!(
                "failed to load model {repo} from Hugging Face: {e}"
            ))
        })?;
        let dim = model.encode_single("dim probe").len();
        Ok(Self {
            model: Arc::new(model),
            dim,
        })
    }

    /// Load the static model from a local directory path.
    ///
    /// Does NOT require network access: `from_pretrained` treats an existing
    /// local path as a vendored model. `normalize = None` honors the model's
    /// `config.json` (`normalize: true`), so vectors are already L2-normalized;
    /// we defensively re-normalize anyway in `embed`.
    pub fn from_dir<P: AsRef<Path>>(dir: P) -> M1ndResult<Self> {
        let dir = dir.as_ref();
        if !dir.exists() {
            return Err(M1ndError::EmbedError(format!(
                "embed: model directory not found at {} (set {} or vendor the model; \
                 the blob is gitignored / Git-LFS managed)",
                dir.display(),
                MODEL_DIR_ENV
            )));
        }
        let model = StaticModel::from_pretrained(dir, None, None, None).map_err(|e| {
            M1ndError::EmbedError(format!("failed to load model from {}: {e}", dir.display()))
        })?;
        // Probe the output dimension once via a trivial encode.
        let probe = model.encode_single("dim probe");
        let dim = probe.len();
        Ok(Self {
            model: Arc::new(model),
            dim,
        })
    }
}

impl Embedder for Model2VecEmbedder {
    fn embed(&self, text: &str) -> Box<[f32]> {
        let mut v = self.model.encode_single(text);
        l2_normalize(&mut v);
        v.into_boxed_slice()
    }

    fn dim(&self) -> usize {
        self.dim
    }
}

/// L2-normalize a vector in place. No-op for zero vectors.
pub fn l2_normalize(v: &mut [f32]) {
    let n = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if n > 0.0 {
        for x in v.iter_mut() {
            *x /= n;
        }
    }
}

/// Cosine similarity between two equal-length vectors. For L2-normalized inputs
/// this equals the dot product. Returns 0.0 on length mismatch or empty input.
pub fn cosine(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let mut dot = 0.0f32;
    let mut na = 0.0f32;
    let mut nb = 0.0f32;
    for i in 0..a.len() {
        dot += a[i] * b[i];
        na += a[i] * a[i];
        nb += b[i] * b[i];
    }
    let denom = na.sqrt() * nb.sqrt();
    if denom > 0.0 {
        dot / denom
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Smoke test: related strings should be more similar than unrelated ones.
    /// Gated on the vendored model file actually existing, since the ~129MB blob
    /// is gitignored / Git-LFS managed and may be absent in CI. When absent, the
    /// test is skipped (printed), NOT failed.
    #[test]
    fn embedder_cosine_smoke() {
        let dir = Model2VecEmbedder::default_model_dir();
        if !dir.join("model.safetensors").exists() {
            eprintln!(
                "SKIP embedder_cosine_smoke: model not vendored at {} (set {} or fetch via Git LFS)",
                dir.display(),
                MODEL_DIR_ENV
            );
            return;
        }
        let emb = Model2VecEmbedder::from_dir(&dir).expect("load model");
        assert!(emb.dim() > 0, "dim must be positive");

        let v_a = emb.embed("graceful shutdown when the task is cancelled");
        let v_b = emb.embed("abort the running job on cancellation");
        let v_c = emb.embed("strawberry cheesecake recipe with whipped cream");

        // L2-normalized => norm ~= 1.
        let norm = v_a.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!(
            (norm - 1.0).abs() < 1e-3,
            "vector should be L2-normalized, got {norm}"
        );

        let related = cosine(&v_a, &v_b);
        let unrelated = cosine(&v_a, &v_c);
        assert!(
            related > unrelated,
            "related ({related}) should exceed unrelated ({unrelated})"
        );
    }
}
