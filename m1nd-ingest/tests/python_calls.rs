use m1nd_core::graph::Graph;
use m1nd_ingest::ownership::source_projection_digest;
use m1nd_ingest::{IngestConfig, Ingestor};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "m1nd-python-call-proof-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&root).unwrap();
        Self(root)
    }

    fn ingest(&self) -> Ingestor {
        Ingestor::new(IngestConfig {
            root: self.0.clone(),
            ..Default::default()
        })
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn has_call(graph: &Graph, caller: &str, callee: &str) -> bool {
    let source = graph.resolve_id(caller).expect("caller must exist");
    let target = graph.resolve_id(callee).expect("callee must exist");
    graph.csr.out_range(source).any(|edge| {
        graph.csr.targets[edge] == target
            && graph.strings.resolve(graph.csr.relations[edge]) == "calls"
    })
}

#[test]
fn python_direct_call_survives_ingest_and_removal_matches_full_rebuild() {
    let fixture = Fixture::new();
    let file = fixture.0.join("module.py");
    fs::write(
        &file,
        "def helper():\n    return 1\n\ndef caller():\n    return helper()\n",
    )
    .unwrap();
    let ingest = fixture.ingest();
    let baseline = ingest.ingest_bundle().unwrap();
    baseline.require_complete().unwrap();
    let caller = "file::module.py::fn::caller";
    let helper = "file::module.py::fn::helper";
    assert!(
        has_call(&baseline.graph, caller, helper),
        "the positive phase must contain a function-to-function call, not just a file-level reference"
    );
    assert!(baseline.ownership.claims_by_source["module.py"]
        .edges
        .iter()
        .any(|edge| edge.source == caller && edge.target == helper && edge.relation == "calls"));

    // Keep both declarations. Only the call disappears; an empty initial graph
    // cannot make the removal assertion pass without the positive proof above.
    fs::write(
        &file,
        "def helper():\n    return 1\n\ndef caller():\n    return 2\n",
    )
    .unwrap();
    // Governed v1 deliberately disables exact-file refresh. Use its supported
    // full-root replacement, as the production CLI warm-refresh path does.
    let removed = ingest.ingest_bundle().unwrap();
    removed.require_complete().unwrap();
    assert!(!has_call(&removed.graph, caller, helper));
    let rebuilt = fixture.ingest().ingest_bundle().unwrap();
    assert_eq!(
        source_projection_digest(&removed.graph).unwrap(),
        source_projection_digest(&rebuilt.graph).unwrap()
    );

    fs::write(&file, "def helper():\n    return 1\n\ndef caller():\n    return helper()\n\ndef second():\n    return helper()\n").unwrap();
    let restored = ingest.ingest_bundle().unwrap();
    restored.require_complete().unwrap();
    assert!(has_call(&restored.graph, caller, helper));
    assert!(has_call(
        &restored.graph,
        "file::module.py::fn::second",
        helper
    ));
    assert_eq!(
        source_projection_digest(&restored.graph).unwrap(),
        source_projection_digest(&fixture.ingest().ingest_bundle().unwrap().graph).unwrap()
    );
}

#[test]
fn python_unknown_bare_call_does_not_bind_to_unique_name_in_another_file() {
    let fixture = Fixture::new();
    fs::write(
        fixture.0.join("module.py"),
        "def caller():\n    return elsewhere()\n",
    )
    .unwrap();
    fs::write(
        fixture.0.join("other.py"),
        "def elsewhere():\n    return 1\n",
    )
    .unwrap();
    let bundle = fixture.ingest().ingest_bundle().unwrap();
    bundle.require_complete().unwrap();
    assert!(!has_call(
        &bundle.graph,
        "file::module.py::fn::caller",
        "file::other.py::fn::elsewhere"
    ));
}
