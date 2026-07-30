// === crates/m1nd-core/src/embed_cache.rs ===
//
// OPTIONAL `embed` feature: a content-addressed, on-disk cache of per-node
// static embeddings, so a warm restart or re-ingest reuses vectors instead of
// recomputing them. Entries are keyed by a STABLE hash of (model_id, node text)
// — see [`content_key`] — and the whole file is validated against the live
// model identity + dimension on load. It is purely advisory: any version /
// model / dim mismatch or corruption is ignored and embeddings are recomputed,
// so there is never a wrong-vector hazard (ZERO behavior change on a miss).
//
// Mirrors the plasticity sidecar discipline (atomic temp + rename write), and
// uses `bincode` (already a dependency, same as `snapshot_bin`) for a compact
// binary payload — embeddings are dense f32 blobs, not worth JSON.

use std::collections::HashMap;
use std::io::{BufWriter, Write};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::{M1ndError, M1ndResult};

/// Same encoding as the graph snapshot: bincode's LEGACY configuration, the only
/// one that reads the cache files already on disk, carrying the same decode
/// budget (`snapshot_bin::DECODE_LIMIT_BYTES`). See `snapshot_bin`'s
/// `SNAPSHOT_BIN_CONFIG` for why `standard()` would corrupt rather than fail, and
/// why an unlimited decode aborts the process on a corrupt length prefix.
///
/// The limit matters MORE here than there. This module's whole contract is that a
/// damaged cache is survivable — "any version / model / dim mismatch or
/// corruption is ignored and embeddings are recomputed" — and a decoder that
/// aborts on garbage breaks that promise in the loudest possible way.
const EMBED_CACHE_CONFIG: bincode::config::Configuration<
    bincode::config::LittleEndian,
    bincode::config::Fixint,
    bincode::config::Limit<{ crate::snapshot_bin::DECODE_LIMIT_BYTES }>,
> = bincode::config::legacy().with_limit::<{ crate::snapshot_bin::DECODE_LIMIT_BYTES }>();

/// Bump when the on-disk layout changes incompatibly. A version mismatch makes
/// [`EmbeddingCache::load_compatible`] return `None` (full recompute).
pub const EMBED_CACHE_VERSION: u32 = 1;

/// On-disk embedding cache. `entries` maps a stable content hash of
/// (model_id, text) to its L2-normalized vector.
#[derive(Serialize, Deserialize)]
pub struct EmbeddingCache {
    /// Layout version — see [`EMBED_CACHE_VERSION`].
    pub version: u32,
    /// Identity of the model that produced these vectors (see
    /// `Model2VecEmbedder::model_id`). A mismatch invalidates the whole file.
    pub model_id: String,
    /// Output dimension of the vectors. A mismatch invalidates the whole file.
    pub dim: u32,
    /// content_key(model_id, text) -> L2-normalized vector.
    pub entries: HashMap<u64, Box<[f32]>>,
}

impl EmbeddingCache {
    /// An empty cache stamped with the current model identity + dimension.
    pub fn new(model_id: String, dim: u32) -> Self {
        Self {
            version: EMBED_CACHE_VERSION,
            model_id,
            dim,
            entries: HashMap::new(),
        }
    }

    /// Load a cache from disk, returning it ONLY if it deserializes cleanly AND
    /// matches the given model identity + dimension. Any I/O error, corruption,
    /// version mismatch, model mismatch, or dim mismatch yields `None`, which
    /// the caller treats as "recompute everything" (advisory cache).
    pub fn load_compatible(path: &Path, model_id: &str, dim: u32) -> Option<Self> {
        let bytes = std::fs::read(path).ok()?;
        // A cache file is exactly one payload: a decode that stops early is a
        // truncated or misencoded file, not a usable cache.
        let (cache, consumed) =
            bincode::serde::decode_from_slice::<EmbeddingCache, _>(&bytes, EMBED_CACHE_CONFIG)
                .ok()?;
        if consumed != bytes.len() {
            return None;
        }
        if cache.version != EMBED_CACHE_VERSION || cache.model_id != model_id || cache.dim != dim {
            return None;
        }
        Some(cache)
    }

    /// Atomically persist the cache (temp + rename), mirroring the plasticity
    /// sidecar's FM-PL-008 discipline. Callers treat failure as non-fatal.
    pub fn save(&self, path: &Path) -> M1ndResult<()> {
        let bytes = bincode::serde::encode_to_vec(self, EMBED_CACHE_CONFIG)
            .map_err(|e| M1ndError::PersistenceFailed(e.to_string()))?;

        let temp_path = path.with_extension("tmp");
        {
            let file = std::fs::File::create(&temp_path)?;
            let mut writer = BufWriter::new(file);
            writer.write_all(&bytes)?;
            writer.flush()?;
        }
        std::fs::rename(&temp_path, path)?;
        Ok(())
    }
}

/// Stable FNV-1a 64-bit hash of (model_id, text). Deterministic across runs,
/// builds, and platforms — unlike `std`'s `DefaultHasher` (random/unspecified
/// seed) — so it is safe to persist as a cache key. A `0xff` separator byte
/// between the two parts prevents boundary-collision aliasing.
pub fn content_key(model_id: &str, text: &str) -> u64 {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    let mut h = OFFSET;
    for &b in model_id.as_bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(PRIME);
    }
    h ^= 0xff;
    h = h.wrapping_mul(PRIME);
    for &b in text.as_bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(PRIME);
    }
    h
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_key_is_deterministic_and_discriminating() {
        let a = content_key("model-x", "graceful shutdown on cancel");
        let b = content_key("model-x", "graceful shutdown on cancel");
        let c = content_key("model-x", "strawberry cheesecake");
        let d = content_key("model-y", "graceful shutdown on cancel");
        assert_eq!(
            a, b,
            "same (model, text) must hash identically across calls"
        );
        assert_ne!(a, c, "different text must (practically) differ");
        assert_ne!(a, d, "different model must (practically) differ");
    }

    #[test]
    fn cache_round_trips_and_validates_identity() {
        let tmp = std::env::temp_dir().join(format!(
            "m1nd_embcache_test_{}.bin",
            content_key("t", "round-trip")
        ));

        let mut cache = EmbeddingCache::new("local/potion-base-8M".to_string(), 256);
        let k = content_key("local/potion-base-8M", "hello world");
        let vec: Box<[f32]> = vec![0.1f32; 256].into_boxed_slice();
        cache.entries.insert(k, vec.clone());
        cache.save(&tmp).expect("save");

        // Exact identity match -> Some, with entries preserved byte-for-byte.
        let loaded = EmbeddingCache::load_compatible(&tmp, "local/potion-base-8M", 256)
            .expect("compatible load");
        assert_eq!(loaded.entries.len(), 1);
        assert_eq!(loaded.entries.get(&k).map(|v| &v[..]), Some(&vec[..]));

        // Model mismatch -> None (recompute).
        assert!(
            EmbeddingCache::load_compatible(&tmp, "other/model", 256).is_none(),
            "model mismatch must invalidate the cache"
        );
        // Dim mismatch -> None (recompute).
        assert!(
            EmbeddingCache::load_compatible(&tmp, "local/potion-base-8M", 512).is_none(),
            "dim mismatch must invalidate the cache"
        );

        let _ = std::fs::remove_file(&tmp);
    }
}
