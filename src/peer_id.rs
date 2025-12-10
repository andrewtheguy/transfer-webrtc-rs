use rand::seq::SliceRandom;
use rand::thread_rng;

const ADJECTIVES: &[&str] = &[
    "happy", "sunny", "brave", "calm", "cool", "cute", "fast", "kind",
    "neat", "nice", "quiet", "smart", "soft", "warm", "wild", "wise",
    "bold", "bright", "clean", "clever", "cozy", "eager", "fair", "fancy",
    "gentle", "glad", "golden", "grand", "great", "jolly", "keen", "lively",
    "lucky", "merry", "mighty", "noble", "proud", "pure", "quick", "rapid",
    "rich", "royal", "sharp", "shiny", "silver", "simple", "smooth", "snowy",
    "spicy", "steady", "strong", "super", "sweet", "swift", "tender", "tiny",
    "vivid", "witty", "young", "zesty",
];

const NOUNS: &[&str] = &[
    "apple", "banana", "cherry", "dolphin", "eagle", "falcon", "grape",
    "harbor", "island", "jungle", "kitten", "lemon", "mango", "nectar",
    "orange", "panda", "quartz", "rabbit", "sunset", "tiger", "umbrella",
    "violet", "walrus", "xenon", "yellow", "zebra", "anchor", "breeze",
    "castle", "dragon", "ember", "forest", "glacier", "horizon", "indigo",
    "jasper", "kraken", "lantern", "meadow", "nebula", "ocean", "phoenix",
    "quasar", "river", "shadow", "thunder", "unicorn", "vortex", "willow",
    "crystal", "dusk", "echo", "flame", "glow", "haze", "iris", "jewel",
    "karma", "lotus", "moon", "nova",
];

/// Generate a human-friendly peer ID like "happy-apple-sunset"
pub fn generate_peer_id() -> String {
    let mut rng = thread_rng();

    let adj1 = ADJECTIVES.choose(&mut rng).unwrap();
    let noun1 = NOUNS.choose(&mut rng).unwrap();
    let noun2 = NOUNS.choose(&mut rng).unwrap();

    format!("{}-{}-{}", adj1, noun1, noun2)
}

/// Validate a peer ID format
pub fn is_valid_peer_id(id: &str) -> bool {
    // Must start and end with alphanumeric, can contain dashes/underscores in middle
    if id.is_empty() || id.len() > 64 {
        return false;
    }

    let chars: Vec<char> = id.chars().collect();

    // First and last must be alphanumeric
    if !chars.first().map(|c| c.is_alphanumeric()).unwrap_or(false) {
        return false;
    }
    if !chars.last().map(|c| c.is_alphanumeric()).unwrap_or(false) {
        return false;
    }

    // All characters must be alphanumeric, dash, or underscore
    chars.iter().all(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_peer_id() {
        let id = generate_peer_id();
        assert!(is_valid_peer_id(&id));
        assert!(id.contains('-'));
    }

    #[test]
    fn test_valid_peer_ids() {
        assert!(is_valid_peer_id("happy-apple-sunset"));
        assert!(is_valid_peer_id("abc123"));
        assert!(is_valid_peer_id("test_id"));
        assert!(is_valid_peer_id("a-b-c"));
    }

    #[test]
    fn test_invalid_peer_ids() {
        assert!(!is_valid_peer_id(""));
        assert!(!is_valid_peer_id("-abc"));
        assert!(!is_valid_peer_id("abc-"));
        assert!(!is_valid_peer_id("ab cd"));
    }
}
