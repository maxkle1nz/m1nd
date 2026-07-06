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
    /// Stable identity of the loaded model (resolved local directory path or
    /// Hugging Face repo id). Recorded into the embedding cache header so a
    /// cache built with a different model is transparently ignored.
    id: String,
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
            id: repo.to_string(),
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
        // Lightweight content fingerprint: fold the weights-file byte length into
        // the identity so an in-place model swap at the same path (different
        // weights) changes `model_id` and invalidates any stale embedding cache.
        // Size is stable (no mtime churn); a same-size, different-weights swap is
        // astronomically unlikely for safetensors.
        let weights_len = std::fs::metadata(dir.join("model.safetensors"))
            .map(|m| m.len())
            .unwrap_or(0);
        Ok(Self {
            model: Arc::new(model),
            dim,
            id: format!("{}#{weights_len}", dir.display()),
        })
    }

    /// Stable identity string of the loaded model (resolved directory path or
    /// Hugging Face repo id). Recorded into the embedding cache header so a
    /// cache produced by a different model is transparently ignored.
    pub fn model_id(&self) -> &str {
        &self.id
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

/// A DETERMINISTIC, model-free [`Embedder`] for tests and CI runs that lack the
/// vendored model blob.
///
/// It hashes the input text (FNV-1a) into a stable seed and expands that seed
/// into a fixed-dimension vector via a small LCG, then L2-normalizes — so the
/// same text ALWAYS maps to the same unit vector, and different texts map to
/// different directions. It is NOT semantically meaningful (no notion of
/// meaning), but it is a faithful stand-in for the *mechanics* the embed path
/// depends on: stable per-text vectors, L2-normalized so `cosine == dot`,
/// content-addressable for the cache. This lets the seek blend, the 0.40 recall
/// floor, cache warm-reuse / self-pruning / single-writer persist, and
/// corruption handling all be proven WITHOUT the ~30 MB blob.
///
/// To make a chosen pair of texts land on a target cosine (e.g. to probe the
/// recall floor), construct with [`FakeEmbedder::with_anchor`], which maps a set
/// of "near" phrases onto one shared base direction plus a small per-text jitter,
/// so their pairwise cosine is high and controllable while unrelated texts stay
/// near-orthogonal.
#[derive(Clone)]
pub struct FakeEmbedder {
    dim: usize,
    /// Texts whose vectors are pulled toward a shared anchor direction (high
    /// mutual cosine), used to drive the recall-floor / blend proofs.
    anchored: Vec<String>,
    /// Blend weight toward the anchor for anchored texts, in [0,1].
    anchor_weight: f32,
}

impl FakeEmbedder {
    /// A plain deterministic embedder of dimension `dim` (no anchoring): every
    /// text maps to its own stable pseudo-random unit vector.
    pub fn new(dim: usize) -> Self {
        Self {
            dim: dim.max(1),
            anchored: Vec::new(),
            anchor_weight: 0.0,
        }
    }

    /// A deterministic embedder where every text in `anchored` is pulled toward
    /// one shared anchor direction by `anchor_weight` (in [0,1]), so anchored
    /// texts share a high mutual cosine (≈ `anchor_weight` for large weight)
    /// while non-anchored texts stay near-orthogonal. Used to place a node's
    /// cosine deliberately above/below the recall floor.
    pub fn with_anchor(dim: usize, anchored: &[&str], anchor_weight: f32) -> Self {
        Self {
            dim: dim.max(1),
            anchored: anchored.iter().map(|s| s.to_string()).collect(),
            anchor_weight: anchor_weight.clamp(0.0, 1.0),
        }
    }

    /// FNV-1a 64-bit hash of the bytes — the stable per-text seed.
    fn seed_of(text: &str) -> u64 {
        let mut h = 0xcbf29ce484222325u64;
        for b in text.as_bytes() {
            h ^= *b as u64;
            h = h.wrapping_mul(0x00000100000001B3);
        }
        h
    }

    /// Expand a seed into a raw (un-normalized), ZERO-MEAN vector via a small
    /// LCG. Centering each vector on its own mean makes two independently-seeded
    /// texts near-orthogonal (expected cosine ≈ 0), so unrelated texts land well
    /// below the recall floor while anchored texts (which share a direction) land
    /// well above it — a controllable, faithful stand-in.
    fn raw_vector(&self, seed: u64) -> Vec<f32> {
        let mut state = seed | 1; // never zero
        let mut v = Vec::with_capacity(self.dim);
        let mut sum = 0.0f32;
        for _ in 0..self.dim {
            // LCG (MMIX by Knuth constants); take the top bits as a value in
            // roughly [0, 2).
            state = state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            let u = (state >> 33) as f32 / (1u64 << 31) as f32; // ~[0, 2)
            v.push(u);
            sum += u;
        }
        // Center on the mean → zero-mean vector (independent seeds ≈ orthogonal).
        let mean = sum / self.dim as f32;
        for x in v.iter_mut() {
            *x -= mean;
        }
        v
    }
}

impl Embedder for FakeEmbedder {
    fn embed(&self, text: &str) -> Box<[f32]> {
        let mut v = self.raw_vector(Self::seed_of(text));
        if self.anchor_weight > 0.0 && self.anchored.iter().any(|a| a == text) {
            // Blend toward a single shared anchor direction so anchored texts
            // have a high, controllable mutual cosine.
            let anchor = self.raw_vector(Self::seed_of("\u{0}m1nd-fake-anchor\u{0}"));
            for (x, a) in v.iter_mut().zip(anchor.iter()) {
                *x = (1.0 - self.anchor_weight) * *x + self.anchor_weight * *a;
            }
        }
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
