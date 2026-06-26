//! Locks the DomainConfig half-life table: per-type overrides, default fallback,
//! and the finite-&-positive invariant that keeps the LN2/half_life decay rate in
//! temporal.rs from dividing by zero or NaN-poisoning. Surfaced by the X-RAY
//! coverage sweep over m1nd-core::domain.
//!
//! Each profile (code/music/memory/light/generic) is checked so that:
//!
//! * a mapped NodeType returns exactly its declared override;
//! * an unmapped NodeType falls back to `default_half_life`;
//! * every returned half-life is finite and strictly positive.

use m1nd_core::domain::DomainConfig;
use m1nd_core::types::NodeType;

/// Every NodeType variant the engine can stamp on a node. `Custom` carries a
/// payload byte, so it is exercised separately below.
const FIXED_NODE_TYPES: &[NodeType] = &[
    NodeType::File,
    NodeType::Directory,
    NodeType::Function,
    NodeType::Class,
    NodeType::Struct,
    NodeType::Enum,
    NodeType::Type,
    NodeType::Module,
    NodeType::Reference,
    NodeType::Concept,
    NodeType::Material,
    NodeType::Process,
    NodeType::Product,
    NodeType::Supplier,
    NodeType::Regulatory,
    NodeType::System,
    NodeType::Cost,
];

/// The decay rate in temporal.rs is `LN2 / half_life`; a zero, negative, NaN, or
/// infinite half-life would divide-by-zero or poison every downstream score.
/// Assert the invariant holds for every node type across every profile.
fn assert_all_half_lives_finite_positive(config: &DomainConfig, profile: &str) {
    for &node_type in FIXED_NODE_TYPES {
        let hl = config.half_life_for(node_type);
        assert!(
            hl.is_finite(),
            "{profile}: half_life_for({node_type:?}) must be finite, got {hl}"
        );
        assert!(
            hl > 0.0,
            "{profile}: half_life_for({node_type:?}) must be > 0, got {hl}"
        );
    }

    // Custom(_) is also a stampable type and must obey the same invariant.
    for payload in [0u8, 7, 255] {
        let node_type = NodeType::Custom(payload);
        let hl = config.half_life_for(node_type);
        assert!(
            hl.is_finite(),
            "{profile}: half_life_for(Custom({payload})) must be finite, got {hl}"
        );
        assert!(
            hl > 0.0,
            "{profile}: half_life_for(Custom({payload})) must be > 0, got {hl}"
        );
    }
}

/// Confirm that each explicitly-mapped (NodeType -> half-life) override is
/// returned verbatim. These constants encode the intended decay schedule for the
/// profile; drifting them silently would change retention behavior.
fn assert_overrides(config: &DomainConfig, profile: &str, overrides: &[(NodeType, f32)]) {
    for &(node_type, expected) in overrides {
        let actual = config.half_life_for(node_type);
        assert_eq!(
            actual, expected,
            "{profile}: half_life_for({node_type:?}) override mismatch"
        );
    }
}

#[test]
fn code_profile_overrides_and_invariants() {
    let config = DomainConfig::code();
    assert_overrides(
        &config,
        "code",
        &[
            (NodeType::File, 168.0),
            (NodeType::Function, 336.0),
            (NodeType::Class, 504.0),
            (NodeType::Struct, 504.0),
            (NodeType::Enum, 504.0),
            (NodeType::Module, 720.0),
            (NodeType::Directory, 720.0),
            (NodeType::Type, 504.0),
        ],
    );
    // Cost is unmapped in the code profile -> must fall back to the default.
    assert_eq!(
        config.half_life_for(NodeType::Cost),
        config.default_half_life,
        "code: unmapped NodeType::Cost must use default_half_life"
    );
    assert_all_half_lives_finite_positive(&config, "code");
}

#[test]
fn music_profile_overrides_and_invariants() {
    let config = DomainConfig::music();
    assert_overrides(
        &config,
        "music",
        &[
            (NodeType::System, 720.0),
            (NodeType::Process, 336.0),
            (NodeType::Material, 168.0),
            (NodeType::Concept, 504.0),
        ],
    );
    // File is unmapped in the music profile -> must fall back to the default.
    assert_eq!(
        config.half_life_for(NodeType::File),
        config.default_half_life,
        "music: unmapped NodeType::File must use default_half_life"
    );
    assert_all_half_lives_finite_positive(&config, "music");
}

#[test]
fn memory_profile_overrides_and_invariants() {
    let config = DomainConfig::memory();
    assert_overrides(
        &config,
        "memory",
        &[
            (NodeType::File, 1008.0),
            (NodeType::Module, 720.0),
            (NodeType::Concept, 720.0),
            (NodeType::Process, 168.0),
            (NodeType::Reference, 336.0),
            (NodeType::System, 840.0),
        ],
    );
    // Function is unmapped in the memory profile -> must fall back to the default.
    assert_eq!(
        config.half_life_for(NodeType::Function),
        config.default_half_life,
        "memory: unmapped NodeType::Function must use default_half_life"
    );
    assert_all_half_lives_finite_positive(&config, "memory");
}

#[test]
fn light_profile_overrides_and_invariants() {
    let config = DomainConfig::light();
    assert_overrides(
        &config,
        "light",
        &[
            (NodeType::File, 1440.0),
            (NodeType::Module, 1080.0),
            (NodeType::Concept, 1080.0),
            (NodeType::Process, 720.0),
            (NodeType::Reference, 840.0),
            (NodeType::System, 1440.0),
        ],
    );
    // Cost is unmapped in the light profile -> must fall back to the default.
    assert_eq!(
        config.half_life_for(NodeType::Cost),
        config.default_half_life,
        "light: unmapped NodeType::Cost must use default_half_life"
    );
    assert_all_half_lives_finite_positive(&config, "light");
}

#[test]
fn generic_profile_uses_default_for_every_type() {
    let config = DomainConfig::generic();
    // The generic profile ships an empty half_lives map: EVERY node type must
    // resolve to default_half_life.
    for &node_type in FIXED_NODE_TYPES {
        assert_eq!(
            config.half_life_for(node_type),
            config.default_half_life,
            "generic: {node_type:?} must resolve to default_half_life (empty map)"
        );
    }
    assert_all_half_lives_finite_positive(&config, "generic");
}

#[test]
fn every_profile_default_half_life_is_finite_positive() {
    let profiles: &[(&str, DomainConfig)] = &[
        ("code", DomainConfig::code()),
        ("music", DomainConfig::music()),
        ("memory", DomainConfig::memory()),
        ("light", DomainConfig::light()),
        ("generic", DomainConfig::generic()),
    ];
    for (profile, config) in profiles {
        assert!(
            config.default_half_life.is_finite(),
            "{profile}: default_half_life must be finite, got {}",
            config.default_half_life
        );
        assert!(
            config.default_half_life > 0.0,
            "{profile}: default_half_life must be > 0, got {}",
            config.default_half_life
        );
    }
}
