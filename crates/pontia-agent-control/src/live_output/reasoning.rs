use super::{LiveOutputItem, LiveOutputUpdate, tools::truncate_text};

/// How a producer publishes a reasoning delta within the per-stream budget.
#[derive(Debug, Clone, PartialEq)]
pub enum LiveReasoningBound {
    /// Publish this (possibly truncated) update.
    Update(LiveOutputUpdate),
    /// The budget is exhausted without room for any delta; publish these items
    /// as a snapshot so the latest reasoning item is marked truncated.
    Snapshot(Vec<LiveOutputItem>),
    /// Reasoning in this stream is already truncated; drop the delta.
    Drop,
}

impl LiveOutputUpdate {
    /// Bound producer-owned reasoning to the per-stream budget enforced by the store.
    /// After any reasoning in the stream is truncated, later reasoning is dropped so
    /// visible reasoning never skips content. Other updates pass through unchanged.
    pub fn bound_reasoning(self, items: &[LiveOutputItem]) -> LiveReasoningBound {
        let LiveOutputUpdate::ReasoningDelta {
            item_id,
            mut delta,
            mut truncated,
        } = self
        else {
            return LiveReasoningBound::Update(self);
        };
        let mut used = 0usize;
        for item in items {
            if let LiveOutputItem::Reasoning {
                text,
                truncated: item_truncated,
                ..
            } = item
            {
                if *item_truncated {
                    return LiveReasoningBound::Drop;
                }
                used += text.len();
            }
        }
        let budget = super::validation::MAX_TOTAL_REASONING_BYTES.saturating_sub(used);
        truncated |= truncate_text(&mut delta, budget);
        if !delta.is_empty() {
            return LiveReasoningBound::Update(LiveOutputUpdate::ReasoningDelta {
                item_id,
                delta,
                truncated,
            });
        }
        // An exhausted budget implies earlier reasoning exists to carry the marker.
        let mut items = items.to_vec();
        match items.iter_mut().rev().find_map(|item| match item {
            LiveOutputItem::Reasoning { truncated, .. } => Some(truncated),
            _ => None,
        }) {
            Some(marker) => {
                *marker = true;
                LiveReasoningBound::Snapshot(items)
            }
            None => LiveReasoningBound::Drop,
        }
    }
}
