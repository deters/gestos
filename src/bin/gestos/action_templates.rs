//! Action templates as the app offers them. Reading them is the library's, as the daemon reads
//! them too, each time a gesture made from one runs.

pub use gestos::action_templates::{find, load, ActionTemplate};
use gestos::config::ActionType;

/// The action a template makes, as the editor and the action search have actions: a link to the
/// template, named as it is, and described as it is, or by what to install for it.
pub fn option(template: &ActionTemplate) -> crate::state::EditorActionOption {
    let tooltip = match template.missing_package() {
        Some(package) => format!("Install {package}"),
        None => template.description.clone(),
    };
    crate::state::EditorActionOption {
        category: 0,
        action_type: ActionType::Template(template.id.clone()),
        name: template.name.clone(),
        tooltip,
    }
}
