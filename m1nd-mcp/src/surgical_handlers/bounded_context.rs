//! Opt-in pagination. The budget is compact JSON of the handler's Value, not
//! the transport envelope or its pretty-print whitespace.

use super::{sha256_hex, surgical, M1ndError, M1ndResult};
use surgical::{
    SurgicalContextContinuation, SurgicalContextInput, SurgicalContextOutput, SurgicalContextPage,
    SurgicalOmissions, SurgicalSourceAnchor,
};

fn invalid(detail: impl Into<String>) -> M1ndError {
    M1ndError::InvalidParams {
        tool: "surgical_context".into(),
        detail: format!(
            "{}; retry without continuation, or increase max_output_bytes",
            detail.into()
        ),
    }
}

/// Use the same Value conversion as dispatch (in particular, f32 numbers).
fn measured_bytes(output: &mut SurgicalContextOutput) -> M1ndResult<usize> {
    loop {
        let bytes = serde_json::to_vec(&serde_json::to_value(&*output)?)?.len();
        let page = output.context_page.as_mut().expect("bounded page");
        if page.serialized_bytes == bytes {
            return Ok(bytes);
        }
        page.serialized_bytes = bytes;
    }
}

fn ordered_neighbours(entries: &mut [surgical::SurgicalNeighbour]) {
    entries.sort_by(|a, b| {
        a.node_id
            .cmp(&b.node_id)
            .then_with(|| a.file_path.cmp(&b.file_path))
            .then_with(|| a.relation.cmp(&b.relation))
            .then_with(|| a.label.cmp(&b.label))
            .then_with(|| a.edge_weight.total_cmp(&b.edge_weight))
    });
}

fn item_count(full: &SurgicalContextOutput) -> usize {
    full.symbols.len()
        + full.callers.len()
        + full.callees.len()
        + full.tests.len()
        + usize::from(full.focused_symbol.is_some())
        + usize::from(full.heuristic_summary.is_some())
}

fn append_item(page: &mut SurgicalContextOutput, full: &SurgicalContextOutput, mut index: usize) {
    if index < full.symbols.len() {
        page.symbols.push(full.symbols[index].clone());
        return;
    }
    index -= full.symbols.len();
    for (source, target) in [
        (&full.callers, &mut page.callers),
        (&full.callees, &mut page.callees),
        (&full.tests, &mut page.tests),
    ] {
        if index < source.len() {
            target.push(source[index].clone());
            return;
        }
        index -= source.len();
    }
    if full.focused_symbol.is_some() {
        if index == 0 {
            page.focused_symbol = full.focused_symbol.clone();
            return;
        }
        index -= 1;
    }
    assert_eq!(index, 0);
    page.heuristic_summary = full.heuristic_summary.clone();
}

struct PageBuilder<'a> {
    full: &'a SurgicalContextOutput,
    max_output_bytes: usize,
    source_sha256: String,
    context_sha256: String,
    graph_generation: u64,
    plasticity_generation: u64,
}

impl PageBuilder<'_> {
    fn page(
        &self,
        source_start: usize,
        source_end: usize,
        item_start: usize,
        item_end: usize,
    ) -> SurgicalContextOutput {
        let full = self.full;
        let source = &full.file_contents;
        let continuation = (source_end < source.len() || item_end < item_count(full)).then(|| {
            SurgicalContextContinuation {
                version: 1,
                context_sha256: self.context_sha256.clone(),
                graph_generation: self.graph_generation,
                plasticity_generation: self.plasticity_generation,
                source_byte_offset: source_end,
                item_offset: item_end,
            }
        });
        let mut page = SurgicalContextOutput {
            file_path: full.file_path.clone(),
            file_contents: source[source_start..source_end].to_string(),
            line_count: full.line_count,
            node_id: full.node_id.clone(),
            symbols: Vec::new(),
            focused_symbol: None,
            callers: Vec::new(),
            callees: Vec::new(),
            tests: Vec::new(),
            heuristic_summary: None,
            elapsed_ms: full.elapsed_ms,
            context_page: None,
        };
        for index in item_start..item_end {
            append_item(&mut page, full, index);
        }
        let (line_start, line_end) = if source_start == source_end {
            (0, 0)
        } else {
            let start = 1 + source[..source_start]
                .bytes()
                .filter(|b| *b == b'\n')
                .count();
            let end = start
                + source.as_bytes()[source_start..source_end - 1]
                    .iter()
                    .filter(|b| **b == b'\n')
                    .count();
            (start, end)
        };
        page.context_page = Some(SurgicalContextPage {
            max_output_bytes: self.max_output_bytes,
            serialized_bytes: 0,
            context_sha256: self.context_sha256.clone(),
            graph_generation: self.graph_generation,
            plasticity_generation: self.plasticity_generation,
            source: SurgicalSourceAnchor {
                sha256: self.source_sha256.clone(),
                total_bytes: source.len(),
                byte_start: source_start,
                byte_end: source_end,
                line_start,
                line_end,
            },
            omitted: SurgicalOmissions {
                source_bytes: source.len() - (source_end - source_start),
                symbols: full.symbols.len() - page.symbols.len(),
                focused_symbol: full.focused_symbol.is_some() && page.focused_symbol.is_none(),
                callers: full.callers.len() - page.callers.len(),
                callees: full.callees.len() - page.callees.len(),
                tests: full.tests.len() - page.tests.len(),
                heuristic_summary: full.heuristic_summary.is_some()
                    && page.heuristic_summary.is_none(),
            },
            continuation,
        });
        page
    }
}

pub(super) fn bounded_page(
    mut full: SurgicalContextOutput,
    input: &SurgicalContextInput,
    graph_generation: u64,
    plasticity_generation: u64,
) -> M1ndResult<SurgicalContextOutput> {
    let Some(budget) = input.max_output_bytes else {
        if input.continuation.is_some() {
            return Err(invalid("continuation requires max_output_bytes"));
        }
        return Ok(full);
    };
    if budget < 1024 {
        return Err(invalid(
            "max_output_bytes must be at least 1024; metadata may require more",
        ));
    }
    ordered_neighbours(&mut full.callers);
    ordered_neighbours(&mut full.callees);
    ordered_neighbours(&mut full.tests);
    let source_sha256 = sha256_hex(full.file_contents.as_bytes());
    // Heuristics are live, time-decayed telemetry, not source snapshot identity.
    let context_sha256 = sha256_hex(&serde_json::to_vec(&serde_json::json!({
        "version": 1, "file_path": full.file_path, "node_id": full.node_id,
        "source_sha256": source_sha256, "symbol": input.symbol,
        "radius": input.radius, "include_tests": input.include_tests,
        "symbols": full.symbols, "focused_symbol": full.focused_symbol,
        "callers": full.callers, "callees": full.callees, "tests": full.tests,
        "graph_generation": graph_generation, "plasticity_generation": plasticity_generation
    }))?);
    let (source_start, item_start) = if let Some(cursor) = &input.continuation {
        if cursor.version != 1
            || cursor.context_sha256 != context_sha256
            || cursor.graph_generation != graph_generation
            || cursor.plasticity_generation != plasticity_generation
        {
            return Err(invalid(
                "stale or mismatched continuation (target, options, source or graph changed)",
            ));
        }
        let source = &full.file_contents;
        if cursor.source_byte_offset > source.len()
            || !source.is_char_boundary(cursor.source_byte_offset)
            || cursor.item_offset > item_count(&full)
            || (cursor.source_byte_offset < source.len() && cursor.item_offset != 0)
            || (cursor.source_byte_offset == source.len()
                && cursor.item_offset == item_count(&full))
        {
            return Err(invalid("invalid continuation offsets"));
        }
        (cursor.source_byte_offset, cursor.item_offset)
    } else {
        (0, 0)
    };
    let builder = PageBuilder {
        full: &full,
        max_output_bytes: budget,
        source_sha256,
        context_sha256,
        graph_generation,
        plasticity_generation,
    };
    let source = &full.file_contents;
    let mut complete = builder.page(source_start, source.len(), item_start, item_count(&full));
    if measured_bytes(&mut complete)? <= budget {
        return Ok(complete);
    }
    let mut output = builder.page(source_start, source_start, item_start, item_start);
    let minimum = measured_bytes(&mut output)?;
    if minimum > budget {
        return Err(invalid(format!(
            "max_output_bytes too small for page metadata; requires at least {minimum}"
        )));
    }
    let mut source_end = source_start;
    // Fit escaped JSON, not raw source length. Search over UTF-8 boundaries.
    let boundaries: Vec<usize> = source[source_start..]
        .char_indices()
        .map(|(index, _)| source_start + index)
        .chain(std::iter::once(source.len()))
        .collect();
    let (mut low, mut high) = (0, boundaries.len());
    while low < high {
        let mid = low + (high - low) / 2;
        let end = boundaries[mid];
        let mut candidate = builder.page(source_start, end, item_start, item_start);
        if measured_bytes(&mut candidate)? <= budget {
            source_end = end;
            output = candidate;
            low = mid + 1;
        } else {
            high = mid;
        }
    }
    let mut item_end = item_start;
    if source_end == source.len() {
        while item_end < item_count(&full) {
            let mut candidate = builder.page(source_start, source_end, item_start, item_end + 1);
            if measured_bytes(&mut candidate)? > budget {
                break;
            }
            output = candidate;
            item_end += 1;
        }
    }
    if source_end == source_start
        && item_end == item_start
        && (source_start < source.len() || item_start < item_count(&full))
    {
        return Err(invalid(
            "max_output_bytes cannot fit the next source character or complete metadata item",
        ));
    }
    Ok(output)
}
