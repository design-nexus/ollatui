/// A prompt command. Aliases such as `/q` still run, but they are not listed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Command {
    pub name: &'static str,
    pub summary: &'static str,
    /// Completing this command inserts a trailing space so an argument can be typed.
    pub takes_arg: bool,
}

pub const COMMANDS: &[Command] = &[
    Command {
        name: "start",
        summary: "Start the Ollama service",
        takes_arg: false,
    },
    Command {
        name: "stop",
        summary: "Stop the Ollama service",
        takes_arg: false,
    },
    Command {
        name: "restart",
        summary: "Restart the Ollama service",
        takes_arg: false,
    },
    Command {
        name: "model",
        summary: "Choose the chat model",
        takes_arg: true,
    },
    Command {
        name: "chats",
        summary: "Show or hide the chat list",
        takes_arg: false,
    },
    Command {
        name: "new",
        summary: "Start a new chat",
        takes_arg: false,
    },
    Command {
        name: "clear",
        summary: "Wipe the current chat",
        takes_arg: false,
    },
    Command {
        name: "cancel",
        summary: "Stop the reply being generated",
        takes_arg: false,
    },
    Command {
        name: "system",
        summary: "Set the system prompt",
        takes_arg: true,
    },
    Command {
        name: "theme",
        summary: "Show or switch the theme",
        takes_arg: true,
    },
    Command {
        name: "pull",
        summary: "Download a model",
        takes_arg: true,
    },
    Command {
        name: "help",
        summary: "Show keys and commands",
        takes_arg: false,
    },
    Command {
        name: "quit",
        summary: "Leave ollatui",
        takes_arg: false,
    },
];

/// Commands matching `query` (the text after `/`), best match first.
/// An empty query returns the catalog in its listed order.
pub fn matching(query: &str) -> Vec<&'static Command> {
    let query = query.trim().trim_start_matches('/').to_ascii_lowercase();
    if query.is_empty() {
        return COMMANDS.iter().collect();
    }
    let mut scored: Vec<(i32, &'static Command)> = COMMANDS
        .iter()
        .filter_map(|command| score(&query, command).map(|score| (score, command)))
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.name.cmp(b.1.name)));
    scored.into_iter().map(|(_, command)| command).collect()
}

/// Names that run a command as typed, including aliases that are not listed.
pub fn is_exact(query: &str) -> bool {
    matches!(query, "q" | "exit" | "help" | "?" | "commands")
        || COMMANDS.iter().any(|command| command.name == query)
}

pub fn completion(command: &Command) -> String {
    if command.takes_arg {
        format!("/{} ", command.name)
    } else {
        format!("/{}", command.name)
    }
}

pub fn name_score(query: &str, name: &str) -> Option<i32> {
    let name = name.to_ascii_lowercase();
    let query = query.to_ascii_lowercase();
    if query.is_empty() {
        return Some(0);
    }
    if name.starts_with(&query) {
        return Some(3_000);
    }
    if let Some(index) = name.find(&query) {
        return Some(2_000 - index as i32);
    }
    subsequence(&name, &query)
}

/// Argument typed after `/model `, if the model list should be open.
pub fn model_argument(input: &str) -> Option<&str> {
    let rest = input.strip_prefix("/model")?;
    if rest.contains('\n') || !rest.starts_with([' ', '\t']) {
        return None;
    }
    Some(rest.trim())
}

fn score(query: &str, command: &Command) -> Option<i32> {
    if let Some(score) = name_score(query, command.name) {
        return Some(score);
    }
    if command.summary.to_ascii_lowercase().contains(query) {
        return Some(1_000);
    }
    None
}

fn subsequence(haystack: &str, query: &str) -> Option<i32> {
    let mut score = 100;
    let mut last = None;
    let mut matched = 0;
    let mut chars = haystack.chars().enumerate();
    for needle in query.chars() {
        let Some((index, _)) = chars.find(|(_, ch)| *ch == needle) else {
            return None;
        };
        if last.is_none() && index == 0 {
            score += 50;
        }
        if let Some(prev) = last {
            if index == prev + 1 {
                score += 20;
            } else {
                score -= (index - prev) as i32;
            }
        }
        last = Some(index);
        matched += 1;
    }
    (matched == query.chars().count()).then_some(score)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_query_lists_every_command() {
        let names: Vec<_> = matching("").iter().map(|c| c.name).collect();
        assert_eq!(names.len(), COMMANDS.len());
        assert_eq!(names[0], "start");
    }

    #[test]
    fn prefix_beats_a_later_substring() {
        let names: Vec<_> = matching("st").iter().map(|c| c.name).collect();
        assert_eq!(names[0], "start");
        assert!(names.contains(&"stop"));
        assert!(names.contains(&"restart"));
    }

    #[test]
    fn fuzzy_subsequence_matches() {
        let names: Vec<_> = matching("srt").iter().map(|c| c.name).collect();
        assert_eq!(names.first().copied(), Some("start"));
        assert!(names.contains(&"restart"));
    }

    #[test]
    fn unknown_text_matches_nothing() {
        assert!(matching("zzzz").is_empty());
    }

    #[test]
    fn model_argument_opens_after_the_space() {
        assert_eq!(model_argument("/model"), None);
        assert_eq!(model_argument("/model "), Some(""));
        assert_eq!(model_argument("/model llama"), Some("llama"));
        assert_eq!(model_argument("/models "), None);
    }

    #[test]
    fn model_names_rank_by_prefix() {
        assert!(name_score("ll", "llama3.2").unwrap() > name_score("ll", "phi").unwrap_or(0));
        assert!(name_score("zzzz", "llama3.2").is_none());
    }

    #[test]
    fn argument_commands_complete_with_a_space() {
        let pull = COMMANDS.iter().find(|c| c.name == "pull").unwrap();
        assert_eq!(completion(pull), "/pull ");
        let quit = COMMANDS.iter().find(|c| c.name == "quit").unwrap();
        assert_eq!(completion(quit), "/quit");
        assert!(is_exact("q"));
        assert!(is_exact("quit"));
        assert!(!is_exact("sta"));
    }
}
