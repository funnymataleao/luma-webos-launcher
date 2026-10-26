//! Source metadata and stable-id selection for streaming wallpapers.
use serde::Deserialize;
use std::{
    collections::BTreeSet,
    hash::{DefaultHasher, Hash, Hasher},
    sync::OnceLock,
};

#[derive(Debug, Deserialize)]
pub struct Video {
    pub name: String,
    pub source: String,
    /// Streamed by the hardware wallpaper on the TV.
    #[cfg(any(test, all(feature = "webos", target_os = "linux", target_arch = "arm")))]
    pub url: String,
    pub id: String,
    pub category: String,
}
pub const ALL_SOURCES: u8 = 0b1111;
pub const FILTERS: [(&str, &str); 5] = [
    ("All", ""),
    ("Apple", "Apple"),
    ("Amazon", "Amazon"),
    ("Aerial Community", "Community (Aerial)"),
    ("RobinFrcd", "Community (RobinFrcd)"),
];

pub fn all() -> &'static [Video] {
    static VIDEOS: OnceLock<Vec<Video>> = OnceLock::new();
    VIDEOS.get_or_init(|| {
        serde_json::from_str(include_str!("video_catalog.json"))
            .expect("The wallpaper catalog must be valid JSON")
    })
}
pub fn source_bit(filter_index: usize) -> Option<u8> {
    match filter_index {
        0 => Some(ALL_SOURCES),
        1..=4 => Some(1 << (filter_index - 1)),
        _ => None,
    }
}
pub fn filtered(mask: u8) -> Vec<usize> {
    all()
        .iter()
        .enumerate()
        .filter_map(|(i, video)| {
            let bit = match video.source.as_str() {
                "Apple" => 1,
                "Amazon" => 2,
                "Community (Aerial)" => 4,
                "Community (RobinFrcd)" => 8,
                _ => 0,
            };
            (mask & bit != 0).then_some(i)
        })
        .collect()
}
pub fn display_filter(filter_index: usize) -> Vec<usize> {
    filtered(source_bit(filter_index).unwrap_or(ALL_SOURCES))
}
pub fn selected(excluded: &BTreeSet<String>) -> Vec<usize> {
    all()
        .iter()
        .enumerate()
        .filter_map(|(i, v)| (!excluded.contains(&v.id)).then_some(i))
        .collect()
}
pub fn selected_count(excluded: &BTreeSet<String>) -> usize {
    all().len() - all().iter().filter(|v| excluded.contains(&v.id)).count()
}
pub fn toggle(excluded: &mut BTreeSet<String>, index: usize) -> bool {
    let Some(video) = all().get(index) else {
        return false;
    };
    if excluded.contains(&video.id) {
        excluded.remove(&video.id)
    } else if selected_count(excluded) > 1 {
        excluded.insert(video.id.clone())
    } else {
        false
    }
}
pub fn groups(filter: usize) -> Vec<(String, String, Vec<usize>)> {
    let mut result: Vec<(String, String, Vec<usize>)> = vec![];
    for index in display_filter(filter) {
        let v = &all()[index];
        if let Some(group) = result
            .iter_mut()
            .find(|g| g.0 == v.source && g.1 == v.category)
        {
            group.2.push(index);
        } else {
            result.push((v.source.clone(), v.category.clone(), vec![index]));
        }
    }
    result
}
#[cfg(any(test, all(feature = "webos", target_os = "linux", target_arch = "arm")))]
pub fn duration_seconds(index: usize) -> Option<f64> {
    static DURATIONS: OnceLock<Vec<Option<f64>>> = OnceLock::new();
    DURATIONS
        .get_or_init(|| {
            serde_json::from_str(include_str!("video_durations.json"))
                .expect("Invalid clip-duration manifest")
        })
        .get(index)
        .copied()
        .flatten()
}
fn shuffled(mut items: Vec<usize>, seed: u64, last: Option<usize>) -> Vec<usize> {
    let mut rng = seed.wrapping_add(0x9e3779b97f4a7c15);
    for i in (1..items.len()).rev() {
        rng ^= rng >> 12;
        rng ^= rng << 25;
        rng ^= rng >> 27;
        let j = (rng.wrapping_mul(0x2545f4914f6cdd1d) % (i + 1) as u64) as usize;
        items.swap(i, j);
    }
    if items.len() > 1 && last == items.first().copied() {
        items.swap(0, 1);
    }
    items
}
pub fn playlist(excluded: &BTreeSet<String>, shuffle: bool, seed: u64) -> Vec<usize> {
    let items = selected(excluded);
    if !shuffle {
        return items;
    }
    if items.len() == 2 {
        return shuffled(items, 0, None);
    }
    let previous_last = if seed == 0 {
        None
    } else {
        shuffled(items.clone(), seed.wrapping_sub(1), None)
            .last()
            .copied()
    };
    shuffled(items, seed, previous_last)
}
pub fn selection_version(excluded: &BTreeSet<String>) -> u64 {
    let mut hasher = DefaultHasher::new();
    excluded.hash(&mut hasher);
    hasher.finish()
}

#[cfg(any(test, all(feature = "webos", target_os = "linux", target_arch = "arm")))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NextChoiceKey {
    pub current: usize,
    pub selection_version: u64,
    pub shuffle: bool,
    pub seed: u64,
}
#[cfg(any(test, all(feature = "webos", target_os = "linux", target_arch = "arm")))]
impl NextChoiceKey {
    pub fn new(current: usize, excluded: &BTreeSet<String>, shuffle: bool, seed: u64) -> Self {
        Self {
            current,
            selection_version: selection_version(excluded),
            shuffle,
            seed,
        }
    }
}

#[cfg(any(test, all(feature = "webos", target_os = "linux", target_arch = "arm")))]
#[derive(Default, Debug)]
pub struct NextChoiceCache {
    key: Option<NextChoiceKey>,
    value: Option<Option<(usize, u64)>>,
}
#[cfg(any(test, all(feature = "webos", target_os = "linux", target_arch = "arm")))]
impl NextChoiceCache {
    #[cfg(all(feature = "webos", target_os = "linux", target_arch = "arm"))]
    pub fn key(&self) -> Option<NextChoiceKey> {
        self.key
    }

    pub fn resolve<F>(&mut self, key: NextChoiceKey, compute: F) -> Option<(usize, u64)>
    where
        F: FnOnce() -> Option<(usize, u64)>,
    {
        if self.key == Some(key) {
            return self.value.flatten();
        }
        let value = compute();
        self.key = Some(key);
        self.value = Some(value);
        value
    }
}

/// Return both the exact prefetch target and the seed used at the next cycle.
pub fn next_choice(
    current: usize,
    excluded: &BTreeSet<String>,
    shuffle: bool,
    seed: u64,
) -> Option<(usize, u64)> {
    let items = playlist(excluded, shuffle, seed);
    if items.is_empty() {
        return None;
    }
    match items.iter().position(|&i| i == current) {
        Some(pos) if pos + 1 < items.len() => Some((items[pos + 1], seed)),
        Some(_) if shuffle => {
            let next_seed = seed.wrapping_add(1);
            Some((
                shuffled(selected(excluded), next_seed, Some(current))[0],
                next_seed,
            ))
        }
        Some(_) => Some((items[0], seed)),
        // If the current item was excluded, restart from the active playlist.
        // This preserves shuffle order instead of falling back to catalog order.
        None => Some((items[0], seed)),
    }
}
pub fn next_index(
    current: usize,
    excluded: &BTreeSet<String>,
    shuffle: bool,
    seed: u64,
) -> Option<usize> {
    next_choice(current, excluded, shuffle, seed).map(|(index, _)| index)
}

/// Previous clip in the active playlist (wraps to the last one).
pub fn prev_index(
    current: usize,
    excluded: &BTreeSet<String>,
    shuffle: bool,
    seed: u64,
) -> Option<usize> {
    let items = playlist(excluded, shuffle, seed);
    let pos = items.iter().position(|&i| i == current)?;
    Some(items[(pos + items.len() - 1) % items.len()])
}

/// Hardware feed schedule, in nanoseconds since the decoder session started.
/// H.264 can be prefetched, but HEVC follows its original decode clock.
#[cfg(any(test, all(feature = "webos", target_os = "linux", target_arch = "arm")))]
pub fn video_feed_schedule_ns(decode_ns: u64, is_h264: bool, lead_ms: u64) -> u64 {
    if is_h264 {
        decode_ns.saturating_sub(lead_ms.saturating_mul(1_000_000))
    } else {
        decode_ns
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn prefetch_duration_metadata_matches_catalog() {
        assert!(duration_seconds(0).is_some_and(|v| v > 60.0));
        assert_eq!(duration_seconds(139), None); // Amazon is untouched
        assert!(duration_seconds(251).is_some_and(|v| v > 45.0));
        assert!(duration_seconds(271).is_some_and(|v| v > 45.0));
        assert_eq!(duration_seconds(289), None);
    }
    #[test]
    fn h264_timing_does_not_wrap_and_amazon_remains_unchanged() {
        assert_eq!(video_feed_schedule_ns(0, true, 250), 0);
        assert_eq!(video_feed_schedule_ns(100_000_000, true, 250), 0);
        assert_eq!(video_feed_schedule_ns(500_000_000, true, 250), 250_000_000);
        assert_eq!(video_feed_schedule_ns(500_000_000, true, 0), 500_000_000);
        assert_eq!(video_feed_schedule_ns(500_000_000, true, 1000), 0);
        assert_eq!(
            video_feed_schedule_ns(500_000_000, false, 1000),
            500_000_000
        );
        assert_eq!(video_feed_schedule_ns(u64::MAX, true, u64::MAX), 0);
    }
    #[test]
    fn metadata() {
        assert_eq!(all().len(), 289);
        assert_eq!(filtered(ALL_SOURCES).len(), 289);
        assert_eq!(filtered(1).len(), 139);
        assert_eq!(filtered(2).len(), 112);
        assert_eq!(filtered(4).len(), 20);
        assert_eq!(filtered(8).len(), 18);
        assert_eq!(filtered(2 | 4).len(), 132);
        assert!(all().iter().all(|v| v.url.starts_with("https://")));
        assert_eq!(
            groups(1)
                .iter()
                .map(|g| (g.1.as_str(), g.2.len()))
                .collect::<Vec<_>>(),
            vec![
                ("Space", 23),
                ("Landscapes", 65),
                ("Cities", 30),
                ("Underwater", 21)
            ]
        );
        assert_eq!(
            all().iter().map(|v| &v.id).collect::<BTreeSet<_>>().len(),
            289
        );
        assert!(duration_seconds(0).is_some_and(|v| v > 60.0));
        assert_eq!(duration_seconds(139), None); // Amazon is untouched
        assert!(duration_seconds(251).is_some_and(|v| v > 45.0));
        assert!(duration_seconds(271).is_some_and(|v| v > 45.0));
        assert_eq!(duration_seconds(289), None);
    }
    #[test]
    fn selection_and_migration() {
        let mut excluded = BTreeSet::new();
        for i in filtered(ALL_SOURCES) {
            if !filtered(4).contains(&i) {
                excluded.insert(all()[i].id.clone());
            }
        }
        assert_eq!(selected_count(&excluded), 20);
        assert!(toggle(&mut excluded, 251));
        assert_eq!(selected_count(&excluded), 19);
        let mut last = BTreeSet::new();
        for i in 1..289 {
            last.insert(all()[i].id.clone());
        }
        assert!(!toggle(&mut last, 0));
    }
    #[test]
    fn shuffle_cycles_and_prefetch_target() {
        let ex = BTreeSet::new();
        let order = playlist(&ex, true, 123);
        assert_eq!(order.len(), 289);
        assert_eq!(order.iter().copied().collect::<BTreeSet<_>>().len(), 289);
        for pair in order.windows(2) {
            assert_eq!(next_index(pair[0], &ex, true, 123), Some(pair[1]));
        }
        let (next, new_seed) = next_choice(*order.last().unwrap(), &ex, true, 123).unwrap();
        assert_eq!(new_seed, 124);
        assert_ne!(next, *order.last().unwrap());
        assert_eq!(
            next_index(*order.last().unwrap(), &ex, true, 123),
            Some(next)
        );
        let mut ex = BTreeSet::new();
        ex.insert(all()[1].id.clone());
        assert_eq!(next_index(0, &ex, false, 0), Some(2));
    }

    #[test]
    fn excluded_current_restarts_from_the_active_shuffled_playlist() {
        let mut excluded = BTreeSet::new();
        excluded.insert(all()[0].id.clone());
        let order = playlist(&excluded, true, 77);
        assert!(!order.is_empty());
        assert_eq!(next_index(0, &excluded, true, 77), Some(order[0]));
    }

    #[test]
    fn next_choice_cache_only_computes_when_its_key_changes() {
        use std::cell::Cell;

        let excluded = BTreeSet::new();
        let key = NextChoiceKey::new(0, &excluded, true, 123);
        let mut cache = NextChoiceCache::default();
        let calls = Cell::new(0);
        let first = cache.resolve(key, || {
            calls.set(calls.get() + 1);
            next_choice(0, &excluded, true, 123)
        });
        let second = cache.resolve(key, || {
            calls.set(calls.get() + 1);
            next_choice(0, &excluded, true, 123)
        });
        assert_eq!(first, second);
        assert_eq!(calls.get(), 1);

        let changed = NextChoiceKey::new(0, &excluded, false, 123);
        let _ = cache.resolve(changed, || {
            calls.set(calls.get() + 1);
            next_choice(0, &excluded, false, 123)
        });
        assert_eq!(calls.get(), 2);
    }
}
