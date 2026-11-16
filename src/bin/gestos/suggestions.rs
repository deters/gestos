use crate::state::EditorActionOption;
use gestos::config::{ActionType, Gesture};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

/// What an action actually does, so that different entries for one action count as the same:
/// the built-in Minimize Window runs GNOME's `minimize`, and `volume-up-static` is `volume-up`.
/// Actions completed by a parameter have no identity of their own, since any number of gestures
/// can use them with different values.
fn action_identity(action: &ActionType) -> Option<String> {
    match action {
        ActionType::Keypress(keys) if keys.is_empty() => None,
        ActionType::Keypress(keys) => Some(format!("keypress:{keys}")),
        ActionType::Keyboard(steps) if steps.is_empty() => None,
        ActionType::Keyboard(steps) => {
            Some(format!("keyboard:{}", gestos::key_steps::format_steps(steps)))
        }
        ActionType::Click(_) => None,
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
    // An app is offered as its own page, for more gestures in it, whether it's opened by one yet
    // or not.
    if crate::ui::app_picker::opened_app(&option.action_type).is_some() {
        return false;
    }
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

/// The generic Run Command in Shell and Custom Action can do what any other action does, so
/// they're always offered, after what the search found.
fn is_open_ended(option: &EditorActionOption) -> bool {
    match &option.action_type {
        ActionType::Execute(command) => command.is_empty(),
        ActionType::Keyboard(steps) => steps.is_empty(),
        ActionType::Keypress(keys) => keys.is_empty(),
        _ => false,
    }
}

// The rank of an open-ended action the search didn't find, which puts it last.
const NOT_FOUND: u8 = u8::MAX;

fn is_listed(option: &EditorActionOption) -> bool {
    match &option.action_type {
        ActionType::Abort => false,
        ActionType::Gnome(key) => !key.ends_with("-static"),
        _ => true,
    }
}

/// Actions matching the search that no gesture uses yet, best first. With nothing to search
/// for, that's every action no gesture uses yet.
pub fn find_suggestions(
    options: &[EditorActionOption],
    gestures: &[Gesture],
    query: &str,
) -> Vec<EditorActionOption> {
    let query = query.trim().to_lowercase();

    let mut matches: Vec<(u8, &EditorActionOption)> = options
        .iter()
        .filter(|option| is_listed(option) && !is_in_use(option, gestures))
        .filter_map(|option| {
            if query.is_empty() {
                Some((0, option))
            } else {
                match_rank(option, &query)
                    .or_else(|| is_open_ended(option).then_some(NOT_FOUND))
                    .map(|rank| (rank, option))
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
        // Keyboard shortcuts stay together after the other actions, with the generic one first.
        let opens_app = crate::ui::app_picker::opened_app(&option.action_type).is_some();
        let (is_shortcut, is_named_shortcut) = match &option.action_type {
            ActionType::Keypress(keys) => (true, !keys.is_empty()),
            _ if opens_app => (false, false),
            ActionType::Keyboard(steps) => (true, !steps.is_empty()),
            _ => (false, false),
        };
        // Gestos's own actions come first, then GNOME's, then one to open each app, as there are
        // many of those.
        let group = match &option.action_type {
            _ if opens_app => 2,
            ActionType::Gnome(_) => 1,
            _ => 0,
        };
        // Of those that make one's own, the simpler shortcut comes before an advanced action.
        let is_advanced = matches!(&option.action_type, ActionType::Keyboard(steps) if steps.is_empty());
        (
            *rank == NOT_FOUND,
            is_shortcut,
            shared_rank,
            group,
            is_named_shortcut,
            is_advanced,
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

/// An entry in the list of what to create a gesture for.
#[derive(Debug, Clone)]
pub enum Suggestion {
    Action(EditorActionOption),
    Category(Category, usize),
}

/// How the actions found are offered: as they are, or in their categories when there are many and
/// nothing is searched for or picked out. A category says how many gestures there are in it, by
/// the categories of the gestures' actions, and shows when it has gestures or actions to offer. In
/// a category, the ones to make one's own of, like a custom keyboard shortcut, a custom action or a
/// command to run, are offered apart, before the rest.
pub fn arrange_suggestions(
    found: Vec<EditorActionOption>,
    searching: bool,
    open_category: Option<Category>,
    gestures_in: &BTreeMap<Category, usize>,
) -> (Vec<EditorActionOption>, Vec<Suggestion>) {
    if let Some(category) = open_category {
        let (apart, in_it): (Vec<_>, Vec<_>) = found
            .into_iter()
            .filter(|option| category_of(option) == category)
            .partition(|option| is_custom_shortcut(option) || is_build_your_own(option));
        return (apart, in_it.into_iter().map(Suggestion::Action).collect());
    }
    if searching {
        return (Vec::new(), found.into_iter().map(Suggestion::Action).collect());
    }
    let ready = found;
    if ready.len() < FEWEST_TO_CATEGORIZE {
        return (Vec::new(), ready.into_iter().map(Suggestion::Action).collect());
    }
    let mut categories: BTreeSet<Category> = ready.iter().map(category_of).collect();
    categories.extend(gestures_in.keys());
    let categories = categories
        .into_iter()
        .map(|category| {
            Suggestion::Category(category, gestures_in.get(&category).copied().unwrap_or(0))
        })
        .collect();
    (Vec::new(), categories)
}

/// The category a gesture is in: the one its action is offered in.
pub fn gesture_category(gesture: &Gesture, options: &[EditorActionOption]) -> Option<Category> {
    let action = gesture.actions.first()?;
    // A gesture for an app is that app's, whatever it does, and is in its window among the apps.
    if !gesture.apps.is_empty() {
        return Some(Category::Apps);
    }
    let category = match options.iter().find(|option| crate::actions::action_matches(action, option)) {
        Some(option) => category_of(option),
        // A command of one's own, which isn't one of GNOME's custom shortcuts.
        None if matches!(action, ActionType::Execute(_)) => Category::Advanced,
        None => category_of(&EditorActionOption {
            category: 0,
            action_type: action.clone(),
            name: String::new(),
            tooltip: String::new(),
        }),
    };
    Some(category)
}

/// How many gestures there are in each category.
pub fn gestures_by_category(
    gestures: &[Gesture],
    options: &[EditorActionOption],
) -> BTreeMap<Category, usize> {
    let mut counts = BTreeMap::new();
    for gesture in gestures.iter().filter(|gesture| !gesture.is_deleted) {
        if let Some(category) = gesture_category(gesture, options) {
            *counts.entry(category).or_default() += 1;
        }
    }
    counts
}

/// Below this many actions to offer, they're listed as they are, as sorting so few into
/// categories would only hide them.
pub const FEWEST_TO_CATEGORIZE: usize = 20;

/// The groups the actions to create gestures for are offered in: GNOME's own, as its Settings
/// list its shortcuts, and Gestos's for the apps and the shortcuts apps share.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Category {
    // First, with the shortcuts apps share, as using apps is what gestures are most for.
    Apps,
    KeyboardShortcuts,
    Windows,
    Navigation,
    Screenshots,
    SoundAndMedia,
    Launchers,
    System,
    Accessibility,
    Typing,
    // Commands and actions of several steps, written out by hand.
    Advanced,
    CustomShortcuts,
    Other,
}

impl Category {
    pub fn name(self) -> &'static str {
        match self {
            Category::Windows => "Windows",
            Category::Navigation => "Navigation",
            Category::Screenshots => "Screenshots",
            Category::SoundAndMedia => "Sound and Media",
            Category::Launchers => "Launchers",
            Category::Apps => "Apps",
            Category::KeyboardShortcuts => "Keyboard Shortcuts",
            Category::System => "System",
            Category::Accessibility => "Accessibility",
            Category::Typing => "Typing",
            Category::Advanced => "Advanced",
            Category::CustomShortcuts => "Custom Shortcuts",
            Category::Other => "Other",
        }
    }

    pub fn icon_name(self) -> &'static str {
        match self {
            Category::Windows => "focus-windows-symbolic",
            Category::Navigation => "preferences-desktop-multitasking-symbolic",
            Category::Screenshots => "camera-photo-symbolic",
            Category::SoundAndMedia => "audio-volume-high-symbolic",
            Category::Launchers => "system-run-symbolic",
            Category::Apps => "view-app-grid-symbolic",
            Category::KeyboardShortcuts => "input-keyboard-symbolic",
            Category::System => "emblem-system-symbolic",
            Category::Accessibility => "preferences-desktop-accessibility-symbolic",
            Category::Typing => "preferences-desktop-locale-symbolic",
            Category::Advanced => "utilities-terminal-symbolic",
            Category::CustomShortcuts => "utilities-terminal-symbolic",
            Category::Other => "view-more-symbolic",
        }
    }
}

/// Whether an action is one the user builds, rather than one that's ready: a custom action, a
/// command or a click. A custom keyboard shortcut goes with the other shortcuts instead.
fn is_build_your_own(option: &EditorActionOption) -> bool {
    match &option.action_type {
        ActionType::Keyboard(steps) => steps.is_empty(),
        ActionType::Execute(command) => command.is_empty(),
        _ => false,
    }
}

/// Whether it's the keyboard shortcut whose keys are recorded, rather than one that's ready.
pub fn is_custom_shortcut(option: &EditorActionOption) -> bool {
    matches!(&option.action_type, ActionType::Keypress(keys) if keys.is_empty())
}

/// The category an action is offered in, as GNOME's Settings sort its shortcuts. Gestos's own
/// actions go where GNOME's for the same thing do.
pub fn category_of(option: &EditorActionOption) -> Category {
    match &option.action_type {
        _ if is_build_your_own(option) => Category::Advanced,
        ActionType::Click(_) | ActionType::PasteSelection => Category::Other,
        _ if crate::ui::app_picker::opened_app(&option.action_type).is_some() => Category::Apps,
        ActionType::Keypress(_) => Category::KeyboardShortcuts,
        // GNOME's own custom shortcuts, which run commands.
        ActionType::Execute(command) if !command.is_empty() => Category::CustomShortcuts,
        ActionType::Gnome(key) => gnome_category(key),
        ActionType::Kde(component, shortcut) => kde_category(component, shortcut),
        ActionType::ToggleTileWindows | ActionType::Lower | ActionType::Raise => Category::Windows,
        ActionType::ControlCenter | ActionType::Lookup => Category::Launchers,
        other => match other.gnome_key() {
            Some(key) => gnome_category(key),
            None => Category::Other,
        },
    }
}

fn gnome_category(key: &str) -> Category {
    let key = key.trim_end_matches("-static");
    let starts = |prefixes: &[&str]| prefixes.iter().any(|prefix| key.starts_with(prefix));
    match key {
        _ if starts(&["screenshot", "window-screenshot", "area-screenshot", "show-screenshot-ui",
            "show-screen-recording-ui"]) => Category::Screenshots,
        _ if starts(&["volume-", "mic-mute", "media", "play", "pause", "stop", "next",
            "previous", "eject"]) => Category::SoundAndMedia,
        "terminal" | "www" | "home" | "email" | "search" | "calculator" | "help"
        | "control-center" => Category::Launchers,
        _ if starts(&["switch-input-source"]) => Category::Typing,
        _ if starts(&["screenreader", "on-screen-keyboard", "magnifier", "increase-text-size",
            "decrease-text-size", "toggle-contrast"]) => Category::Accessibility,
        _ if starts(&["switch-to-workspace", "move-to-workspace", "switch-windows",
            "cycle-windows", "switch-group", "cycle-group", "switch-panels", "cycle-panels",
            "switch-applications", "switch-to-application", "move-to-monitor", "show-desktop",
            "shift-overview"]) => Category::Navigation,
        _ if starts(&["close", "minimize", "maximize", "unmaximize", "toggle-maximized",
            "toggle-fullscreen", "toggle-tiled", "toggle-above", "toggle-on-all-workspaces",
            "always-on-top", "raise", "lower", "begin-move", "begin-resize", "move-to-side",
            "move-to-corner", "move-to-center", "activate-window-menu",
            "open-new-window-application"]) => Category::Windows,
        _ => Category::System,
    }
}

/// KDE's shortcuts, by the part of KDE they belong to, and for KWin, by what they do.
fn kde_category(component: &str, shortcut: &str) -> Category {
    let shortcut = shortcut.to_lowercase();
    match component {
        "kwin" if ["desktop", "switch", "walk", "overview", "expose", "grid"]
            .iter()
            .any(|word| shortcut.contains(word)) => Category::Navigation,
        "kwin" => Category::Windows,
        "kmix" | "mediacontrol" => Category::SoundAndMedia,
        "org_kde_spectacle_desktop" | "org.kde.spectacle.desktop" => Category::Screenshots,
        "kaccess" => Category::Accessibility,
        "KDE Keyboard Layout Switcher" => Category::Typing,
        "ksmserver" | "plasmashell" | "org_kde_powerdevil" | "kded5" => Category::System,
        _ if component.ends_with(".desktop") || component.ends_with("_desktop") => {
            Category::Launchers
        }
        _ => Category::Other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn actions_go_where_gnome_puts_its_own() {
        let category = |action: ActionType| category_of(&option("", action));
        assert_eq!(category(ActionType::Gnome("close".into())), Category::Windows);
        assert_eq!(category(ActionType::Kill), Category::Windows);
        assert_eq!(category(ActionType::WorkspaceLeft), Category::Navigation);
        assert_eq!(
            category(ActionType::Gnome("move-to-workspace-left".into())),
            Category::Navigation
        );
        assert_eq!(category(ActionType::Gnome("volume-up-static".into())), Category::SoundAndMedia);
        assert_eq!(category(ActionType::MediaNext), Category::SoundAndMedia);
        assert_eq!(category(ActionType::ScreenshotArea), Category::Screenshots);
        assert_eq!(category(ActionType::Calculator), Category::Launchers);
        assert_eq!(category(ActionType::LockScreen), Category::System);
        assert_eq!(category(ActionType::ShowOverview), Category::System);
        assert_eq!(category(ActionType::Gnome("magnifier".into())), Category::Accessibility);
        assert_eq!(category(ActionType::Gnome("switch-input-source".into())), Category::Typing);
        assert_eq!(category(ActionType::Keypress("Control+T".into())), Category::KeyboardShortcuts);
        assert_eq!(category(ActionType::Execute("backup.sh".into())), Category::CustomShortcuts);
        assert_eq!(
            category(ActionType::Kde("kwin".into(), "Switch to Desktop 2".into())),
            Category::Navigation
        );
        assert_eq!(category(ActionType::Kde("kwin".into(), "Window Close".into())), Category::Windows);
    }

    fn names_of(suggestions: &[Suggestion]) -> Vec<String> {
        suggestions
            .iter()
            .map(|suggestion| match suggestion {
                Suggestion::Action(option) => option.name.clone(),
                Suggestion::Category(category, count) => format!("{} {count}", category.name()),
            })
            .collect()
    }

    fn many_actions() -> Vec<EditorActionOption> {
        let mut options = vec![option("Advanced Action", ActionType::Keyboard(Vec::new()))];
        options.extend((0..15).map(|i| gnome(&format!("Workspace {i}"), &format!("switch-to-workspace-{i}"))));
        options.extend((0..5).map(|i| gnome(&format!("Volume {i}"), "volume-up")));
        options
    }

    #[test]
    fn many_actions_are_offered_in_categories() {
        let (apart, others) = arrange_suggestions(many_actions(), false, None, &BTreeMap::new());
        assert!(apart.is_empty());
        assert_eq!(names_of(&others), ["Navigation 0", "Sound and Media 0", "Advanced 0"]);
    }

    #[test]
    fn a_category_shows_its_actions_and_a_search_shows_them_all() {
        let (builds, others) =
            arrange_suggestions(many_actions(), false, Some(Category::SoundAndMedia), &BTreeMap::new());
        assert!(builds.is_empty());
        assert_eq!(names_of(&others).len(), 5);
        let (builds, others) = arrange_suggestions(many_actions(), true, None, &BTreeMap::new());
        assert!(builds.is_empty());
        assert_eq!(others.len(), 21);
    }

    #[test]
    fn the_custom_keyboard_shortcut_comes_apart_in_its_category() {
        let mut options: Vec<_> = (0..20)
            .map(|i| option(&format!("Shortcut {i}"), ActionType::Keypress(format!("Control+{i}"))))
            .collect();
        options.push(option("Custom Keyboard Shortcut", ActionType::Keypress(String::new())));
        let (builds, others) = arrange_suggestions(options.clone(), false, None, &BTreeMap::new());
        assert!(builds.is_empty());
        assert_eq!(names_of(&others), ["Keyboard Shortcuts 0"]);
        let (apart, others) =
            arrange_suggestions(options, false, Some(Category::KeyboardShortcuts), &BTreeMap::new());
        assert_eq!(names(&apart), ["Custom Keyboard Shortcut"]);
        assert_eq!(others.len(), 20);
    }

    #[test]
    fn an_advanced_action_and_a_command_come_apart_in_advanced() {
        let options = vec![
            option("Open Files", ActionType::Execute("nautilus".into())),
            option("Advanced Action", ActionType::Keyboard(Vec::new())),
            option("Run Command in Shell", ActionType::Execute(String::new())),
        ];
        let (apart, _) = arrange_suggestions(options, false, Some(Category::Advanced), &BTreeMap::new());
        assert_eq!(names(&apart), ["Advanced Action", "Run Command in Shell"]);
    }

    #[test]
    fn gestures_are_in_their_actions_categories_and_counted_there() {
        let options = vec![
            option("Close Window", ActionType::Kill),
            option("Advanced Action", ActionType::Keyboard(Vec::new())),
        ];
        let mut steps = gesture(ActionType::Keyboard(vec![
            gestos::key_steps::KeyStep::Type("hello".into()),
        ]));
        steps.name = "Say hello".into();
        let gestures = [gesture(ActionType::Kill), gesture(ActionType::Kill), steps];
        assert_eq!(gesture_category(&gestures[0], &options), Some(Category::Windows));
        assert_eq!(gesture_category(&gestures[2], &options), Some(Category::Advanced));
        let mut command = gesture(ActionType::Execute("backup.sh".into()));
        command.name = "Back up".into();
        assert_eq!(gesture_category(&command, &options), Some(Category::Advanced));
        let counts = gestures_by_category(&gestures, &options);
        assert_eq!(counts.get(&Category::Windows), Some(&2));
        assert_eq!(counts.get(&Category::Advanced), Some(&1));
        // A category with gestures and nothing left to offer still shows, with its count.
        let (_, categories) = arrange_suggestions(many_actions(), false, None, &counts);
        assert_eq!(
            names_of(&categories),
            ["Windows 2", "Navigation 0", "Sound and Media 0", "Advanced 1"]
        );
    }

    #[test]
    fn a_few_actions_are_not_put_in_categories() {
        let mut few = many_actions();
        few.truncate(10);
        let (apart, others) = arrange_suggestions(few, false, None, &BTreeMap::new());
        assert!(apart.is_empty());
        assert_eq!(names_of(&others)[..2], ["Advanced Action", "Workspace 0"]);
    }

    #[test]
    fn a_gesture_for_an_app_is_among_the_apps() {
        let options = [
            option("Copy", ActionType::Keypress("Control+C".into())),
            option("Close Window", ActionType::Kill),
        ];
        let mut copy = gesture(ActionType::Keypress("Control+C".into()));
        assert_eq!(gesture_category(&copy, &options), Some(Category::KeyboardShortcuts));
        copy.apps = vec!["org.gnome.Calculator".into()];
        assert_eq!(gesture_category(&copy, &options), Some(Category::Apps));
        let mut close = gesture(ActionType::Kill);
        close.apps = vec!["org.mozilla.firefox".into()];
        assert_eq!(gesture_category(&close, &options), Some(Category::Apps));
    }

    #[test]
    fn actions_to_build_are_advanced() {
        let category = |action: ActionType| category_of(&option("", action));
        assert_eq!(category(ActionType::Keyboard(Vec::new())), Category::Advanced);
        assert_eq!(category(ActionType::Execute(String::new())), Category::Advanced);
        assert_eq!(category(ActionType::Click(None)), Category::Other);
        assert_eq!(category(ActionType::Keypress(String::new())), Category::KeyboardShortcuts);
        assert_eq!(category(ActionType::Execute("backup.sh".into())), Category::CustomShortcuts);
    }

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
            apps: Vec::new(),
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
            option("Run Command in Shell", ActionType::Execute(String::new())),
            option("Keyboard Action", ActionType::Keyboard(Vec::new())),
        ];
        let gestures = [
            gesture(ActionType::Execute("firefox".to_string())),
            gesture(ActionType::Keypress("Alt+Left".to_string())),
        ];
        assert_eq!(
            names(&find_suggestions(&options, &gestures, "run")),
            ["Run Command in Shell", "Keyboard Action"]
        );
        assert_eq!(
            names(&find_suggestions(&options, &gestures, "keyboard")),
            ["Keyboard Action", "Run Command in Shell"]
        );
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
    fn an_empty_search_lists_only_the_actions_not_in_use() {
        let mut options = minimize_options();
        options.push(gnome("Volume up", "volume-up"));
        let gestures = [gesture(ActionType::Iconify)];
        assert_eq!(names(&find_suggestions(&options, &gestures, "   ")), ["Volume up"]);
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
    fn ones_to_make_your_own_are_always_offered_last() {
        let options = [
            option("Copy", ActionType::Keypress("Control+C".to_string())),
            option("Advanced Action", ActionType::Keyboard(Vec::new())),
            option("Custom Keyboard Shortcut", ActionType::Keypress(String::new())),
            option("Run Command in Shell", ActionType::Execute(String::new())),
            option("Close Window", ActionType::Kill),
        ];
        assert_eq!(
            names(&find_suggestions(&options, &[], "close")),
            ["Close Window", "Run Command in Shell", "Custom Keyboard Shortcut", "Advanced Action"]
        );
        assert_eq!(
            names(&find_suggestions(&options, &[], "advanced")),
            ["Advanced Action", "Run Command in Shell", "Custom Keyboard Shortcut"]
        );
    }

    #[test]
    fn opening_an_app_is_listed_after_the_desktops_actions() {
        let open = |app: &str| {
            ActionType::Keyboard(vec![gestos::key_steps::KeyStep::Open(app.to_string())])
        };
        let options = [
            option("Copy", ActionType::Keypress("Control+C".to_string())),
            option("Open Text Editor", open("org.gnome.TextEditor")),
            option("Close Window", ActionType::Kill),
            gnome("Volume up", "volume-up"),
        ];
        assert_eq!(
            names(&find_suggestions(&options, &[], "")),
            ["Close Window", "Volume up", "Open Text Editor", "Copy"]
        );
        // Still offered once a gesture opens it, as its page is where more gestures for it go.
        let used = [gesture(open("org.gnome.TextEditor"))];
        assert_eq!(
            names(&find_suggestions(&options, &used, "")),
            ["Close Window", "Volume up", "Open Text Editor", "Copy"]
        );
    }

    #[test]
    fn keyboard_shortcuts_stay_together_after_the_other_actions() {
        let keys = |keys: &str| ActionType::Keypress(keys.to_string());
        let options = [
            option("Copy", keys("Control+C")),
            option("Keyboard Action", ActionType::Keyboard(Vec::new())),
            option("Zoom In", keys("Control+plus")),
            option("Close Window", ActionType::Kill),
            gnome("Volume up", "volume-up"),
        ];
        assert_eq!(
            names(&find_suggestions(&options, &[], "")),
            ["Close Window", "Volume up", "Keyboard Action", "Copy", "Zoom In"]
        );

        let close = [
            option("Close (Ctrl+W)", keys("Control+W")),
            option("Maybe Close", ActionType::Kill),
        ];
        assert_eq!(
            names(&find_suggestions(&close, &[], "close")),
            ["Maybe Close", "Close (Ctrl+W)"]
        );
    }

    #[test]
    fn a_standard_shortcut_in_use_is_no_longer_suggested() {
        let copy = option("Copy", ActionType::Keypress("Control+C".to_string()));
        let used = [gesture(ActionType::Keypress("Control+C".to_string()))];
        let other = [gesture(ActionType::Keypress("Control+V".to_string()))];
        assert!(find_suggestions(std::slice::from_ref(&copy), &used, "copy").is_empty());
        assert_eq!(find_suggestions(&[copy], &other, "copy").len(), 1);
    }
}
