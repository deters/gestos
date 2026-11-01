use crate::state::EditorActionOption;
use gestos::config::{ActionType, Gesture};
use std::collections::{HashMap, HashSet};

/// What an action actually does, so that different entries for one action count as the same:
/// the built-in Minimize Window runs GNOME's `minimize`, and `volume-up-static` is `volume-up`.
/// Actions completed by a parameter have no identity of their own, since any number of gestures
/// can use them with different values.
fn action_identity(action: &ActionType) -> Option<String> {
    match action {
        ActionType::Keypress(_) | ActionType::Click(_) => None,
        ActionType::Execute(command) if command.is_empty() => None,
        ActionType::Execute(command) => Some(format!("execute:{command}")),
        ActionType::Gnome(key) => Some(format!("gnome:{}", key.trim_end_matches("-static"))),
        other => match other.gnome_key() {
            Some(key) => Some(format!("gnome:{key}")),
            None => Some(other.to_string()),
        },
    }
}

fn is_in_use(option: &EditorActionOption, gestures: &[Gesture]) -> bool {
    let Some(identity) = action_identity(&option.action_type) else {
        return false;
    };
    gestures
        .iter()
        .filter(|gesture| !gesture.is_deleted)
        .flat_map(|gesture| &gesture.actions)
        .any(|action| action_identity(action).as_ref() == Some(&identity))
}

/// Whether a search matches some text, ignoring case. A space works like the regular expression
/// `.*`: the words must appear in that order, with anything between them.
pub fn search_matches(text: &str, query: &str) -> bool {
    let text = text.to_lowercase();
    let query = query.to_lowercase();
    let mut remaining = text.as_str();
    for word in query.split_whitespace() {
        match remaining.find(word) {
            Some(position) => remaining = &remaining[position + word.len()..],
            None => return false,
        }
    }
    true
}

fn text_rank(text: &str, query: &str) -> Option<u8> {
    if !search_matches(text, query) {
        return None;
    }
    let text = text.to_lowercase();
    // With several words, how well the first one fits decides.
    let first_word = query.split_whitespace().next().unwrap_or(query);
    if text == query {
        Some(0)
    } else if text.starts_with(first_word) {
        Some(1)
    } else if text
        .split(|c: char| !c.is_alphanumeric())
        .any(|word| word.starts_with(first_word))
    {
        Some(2)
    } else {
        Some(3)
    }
}

/// Lower is better. The internal key is searched too, because GNOME's titles can be translated.
fn match_rank(option: &EditorActionOption, query: &str) -> Option<u8> {
    let name_rank = text_rank(&option.name, query);
    let key_rank = match &option.action_type {
        ActionType::Gnome(key) => text_rank(key, query),
        _ => None,
    };
    match (name_rank, key_rank) {
        (Some(name), Some(key)) => Some(name.min(key)),
        (name, key) => name.or(key),
    }
}

fn is_listed(option: &EditorActionOption) -> bool {
    match &option.action_type {
        ActionType::Abort => false,
        ActionType::Gnome(key) => !key.ends_with("-static"),
        _ => true,
    }
}

/// Actions matching the search that no gesture uses yet, best first. With nothing to search
/// for, that's every action if there are no gestures to show instead, and otherwise nothing.
pub fn find_suggestions(
    options: &[EditorActionOption],
    gestures: &[Gesture],
    query: &str,
) -> Vec<EditorActionOption> {
    let query = query.trim().to_lowercase();
    let has_gestures = gestures.iter().any(|gesture| !gesture.is_deleted);
    if query.is_empty() && has_gestures {
        return Vec::new();
    }

    let mut matches: Vec<(u8, &EditorActionOption)> = options
        .iter()
        .filter(|option| is_listed(option) && !is_in_use(option, gestures))
        .filter_map(|option| {
            if query.is_empty() {
                Some((0, option))
            } else {
                match_rank(option, &query).map(|rank| (rank, option))
            }
        })
        .collect();
    // Entries for one action share their best rank, so that the friendlier built-in name is the
    // one kept even if only GNOME's key matched exactly.
    let mut best_ranks: HashMap<String, u8> = HashMap::new();
    for (rank, option) in &matches {
        if let Some(identity) = action_identity(&option.action_type) {
            let best = best_ranks.entry(identity).or_insert(*rank);
            *best = (*best).min(*rank);
        }
    }
    matches.sort_by_cached_key(|(rank, option)| {
        let shared_rank = action_identity(&option.action_type)
            .and_then(|identity| best_ranks.get(&identity).copied())
            .unwrap_or(*rank);
        (
            shared_rank,
            matches!(option.action_type, ActionType::Gnome(_)),
            option.name.to_lowercase(),
        )
    });

    let mut seen = HashSet::new();
    matches
        .into_iter()
        .filter(|(_, option)| match action_identity(&option.action_type) {
            Some(identity) => seen.insert(identity),
            None => true,
        })
        .map(|(_, option)| option.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn option(name: &str, action_type: ActionType) -> EditorActionOption {
        EditorActionOption {
            category: 0,
            action_type,
            name: name.to_string(),
            tooltip: String::new(),
        }
    }

    fn gnome(name: &str, key: &str) -> EditorActionOption {
        option(name, ActionType::Gnome(key.to_string()))
    }

    fn gesture(action: ActionType) -> Gesture {
        Gesture {
            id: "id".to_string(),
            name: "gesture".to_string(),
            raw_movement: String::new(),
            points: Vec::new(),
            actions: vec![action],
            is_custom: true,
            is_modified: false,
            is_deleted: false,
            modifiers: Default::default(),
        }
    }

    fn names(suggestions: &[EditorActionOption]) -> Vec<&str> {
        suggestions.iter().map(|s| s.name.as_str()).collect()
    }

    fn minimize_options() -> Vec<EditorActionOption> {
        vec![
            gnome("Minimize window", "minimize"),
            option("Minimize Window", ActionType::Iconify),
        ]
    }

    #[test]
    fn a_builtin_and_its_gnome_action_appear_once_with_the_builtin_first() {
        let found = find_suggestions(&minimize_options(), &[], "minimize");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].action_type, ActionType::Iconify);
    }

    #[test]
    fn a_gesture_using_either_form_means_no_suggestion() {
        for action in [ActionType::Iconify, ActionType::Gnome("minimize".to_string())] {
            let gestures = [gesture(action)];
            assert!(find_suggestions(&minimize_options(), &gestures, "minimize").is_empty());
        }
    }

    #[test]
    fn a_deleted_gesture_does_not_count_as_using_an_action() {
        let mut deleted = gesture(ActionType::Iconify);
        deleted.is_deleted = true;
        assert_eq!(find_suggestions(&minimize_options(), &[deleted], "minimize").len(), 1);
    }

    #[test]
    fn static_variants_and_abort_are_not_suggested() {
        let options = vec![
            gnome("Volume up", "volume-up"),
            gnome("Volume up", "volume-up-static"),
            option("Abort Gesture", ActionType::Abort),
        ];
        assert_eq!(names(&find_suggestions(&options, &[], "volume")), ["Volume up"]);
        assert!(find_suggestions(&options, &[], "abort").is_empty());
    }

    #[test]
    fn a_static_gesture_counts_as_using_the_action() {
        let options = vec![gnome("Volume up", "volume-up")];
        let gestures = [gesture(ActionType::Gnome("volume-up-static".to_string()))];
        assert!(find_suggestions(&options, &gestures, "volume").is_empty());
    }

    #[test]
    fn the_internal_key_matches_a_translated_title() {
        let options = vec![gnome("Minimizar janela", "minimize")];
        assert_eq!(names(&find_suggestions(&options, &[], "minimize")), ["Minimizar janela"]);
    }

    #[test]
    fn closer_matches_rank_first() {
        let options = vec![
            gnome("Take a screenshot of a window", "screenshot-window"),
            option("Prescreen", ActionType::Www),
            gnome("Screen", "screen"),
            gnome("Screen reader", "screenreader"),
        ];
        assert_eq!(
            names(&find_suggestions(&options, &[], "screen")),
            ["Screen", "Screen reader", "Take a screenshot of a window", "Prescreen"]
        );
    }

    #[test]
    fn parameter_actions_stay_available_however_often_they_are_used() {
        let options = vec![
            option("Run Command (Execute)", ActionType::Execute(String::new())),
            option("Keyboard Shortcut", ActionType::Keypress(String::new())),
        ];
        let gestures = [
            gesture(ActionType::Execute("firefox".to_string())),
            gesture(ActionType::Keypress("Alt+Left".to_string())),
        ];
        assert_eq!(find_suggestions(&options, &gestures, "run").len(), 1);
        assert_eq!(find_suggestions(&options, &gestures, "keyboard").len(), 1);
    }

    #[test]
    fn a_custom_shortcut_is_in_use_only_with_its_own_command() {
        let options = vec![option("Browser", ActionType::Execute("firefox".to_string()))];
        let same = [gesture(ActionType::Execute("firefox".to_string()))];
        let other = [gesture(ActionType::Execute("gedit".to_string()))];
        assert!(find_suggestions(&options, &same, "browser").is_empty());
        assert_eq!(find_suggestions(&options, &other, "browser").len(), 1);
    }

    #[test]
    fn spaces_match_anything_between_words_in_order() {
        assert!(search_matches("Take a screenshot of a window", "take window"));
        assert!(search_matches("Take a screenshot of a window", "SCREEN WIN"));
        assert!(!search_matches("Take a screenshot of a window", "window take"));
        assert!(!search_matches("Take a screenshot", "take window"));
        assert!(search_matches("anything", "   "));
    }

    #[test]
    fn a_repeated_word_must_appear_twice() {
        assert!(search_matches("a window, another window", "window window"));
        assert!(!search_matches("one window", "window window"));
    }

    #[test]
    fn regular_expression_characters_are_literal() {
        assert!(!search_matches("Close window", "c.ose"));
        assert!(search_matches("Press (Alt)", "(alt)"));
    }

    #[test]
    fn suggestions_match_words_in_order_in_a_name_or_key() {
        let options = vec![
            gnome("Move window one workspace to the left", "move-to-workspace-left"),
            gnome("Switch to workspace on the left", "switch-to-workspace-left"),
        ];
        assert_eq!(
            names(&find_suggestions(&options, &[], "move left")),
            ["Move window one workspace to the left"]
        );
        assert_eq!(find_suggestions(&options, &[], "to left").len(), 2);
        assert!(find_suggestions(&options, &[], "left move").is_empty());
    }

    #[test]
    fn an_empty_search_suggests_nothing_when_there_are_gestures() {
        let gestures = [gesture(ActionType::Kill)];
        assert!(find_suggestions(&minimize_options(), &gestures, "   ").is_empty());
    }

    #[test]
    fn with_no_gestures_an_empty_search_lists_every_action_once() {
        let mut options = minimize_options();
        options.push(gnome("Volume up", "volume-up"));
        options.push(gnome("Volume up", "volume-up-static"));
        options.push(option("Abort Gesture", ActionType::Abort));
        assert_eq!(
            names(&find_suggestions(&options, &[], "  ")),
            ["Minimize Window", "Volume up"]
        );
    }

    #[test]
    fn deleted_gestures_do_not_count_as_having_gestures() {
        let mut deleted = gesture(ActionType::Kill);
        deleted.is_deleted = true;
        assert_eq!(find_suggestions(&minimize_options(), &[deleted], "").len(), 1);
    }
}
