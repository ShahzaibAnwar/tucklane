pub mod execute;
pub mod plan;

pub use execute::{execute, ExecItem, ExecReport, Progress};
pub use plan::{build_plan, plan_entries, Plan, PlanInputs, PlanItem, PlanMode, PlanStatus};

use crate::fsops::OnCollision;
use crate::settings::CollisionPolicy;

/// Turns selected plan items into executable moves.
pub fn exec_items_from_plan<'a>(
    items: impl IntoIterator<Item = &'a PlanItem>,
    collision: CollisionPolicy,
) -> Vec<ExecItem> {
    items
        .into_iter()
        .filter_map(|i| {
            let dest_dir = i.dest_dir.clone()?;
            Some(ExecItem {
                source_rel: i.source_rel.clone(),
                name: i.name.clone(),
                size: i.size,
                modified_ms: i.modified_ms,
                dest_dir,
                dest_name: i.dest_name.clone().unwrap_or_else(|| i.name.clone()),
                // "Skip" means: never rename. Everything else may pick a free name
                // if the destination got taken after the preview.
                on_collision: if collision == CollisionPolicy::Skip {
                    OnCollision::Fail
                } else {
                    OnCollision::Rename
                },
                rule_id: i.classification.rule_id.clone(),
                rule_name: i.classification.rule_name.clone(),
                reason: Some(i.classification.reason.clone()),
            })
        })
        .collect()
}
