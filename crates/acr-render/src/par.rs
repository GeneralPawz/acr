//! Per-item loops of the 3D frame spread over the cores (rayon, native); in order on the one
//! thread of the browser. The results keep the items' order, so draw lists and frames are the
//! same as a sequential loop's.

/// Below this many items a loop stays on the calling thread (handing out costs more).
pub const MIN_ITEMS: usize = 64;

/// `items.iter().map(f).collect()`, in parallel on native once there are [`MIN_ITEMS`].
#[cfg(not(target_arch = "wasm32"))]
pub fn map<T: Sync, R: Send>(items: &[T], f: impl Fn(&T) -> R + Sync + Send) -> Vec<R> {
    use rayon::prelude::*;
    if items.len() < MIN_ITEMS {
        items.iter().map(f).collect()
    } else {
        items.par_iter().with_min_len(16).map(f).collect()
    }
}

/// `items.iter().map(f).collect()` (wasm: single-threaded).
#[cfg(target_arch = "wasm32")]
pub fn map<T, R>(items: &[T], f: impl Fn(&T) -> R) -> Vec<R> {
    items.iter().map(f).collect()
}

#[cfg(test)]
mod tests {
    #[test]
    fn map_keeps_order() {
        let items: Vec<u32> = (0..1000).collect();
        let out = super::map(&items, |&i| i * 3);
        assert_eq!(out, items.iter().map(|&i| i * 3).collect::<Vec<_>>());
    }
}
