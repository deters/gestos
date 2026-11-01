//! The data passed between the daemon and the gesture chooser it launches.

use crate::config::parse_points;
use crate::protractor::Point2D;

#[derive(Debug, Clone, PartialEq)]
pub struct ChooserEntry {
    pub name: String,
    pub description: String,
    pub points: Vec<Point2D>,
}

fn one_line(text: &str) -> String {
    text.replace(['\t', '\n', '\r'], " ")
}

pub fn encode_entries(entries: &[ChooserEntry]) -> String {
    entries
        .iter()
        .map(|entry| {
            let points = entry
                .points
                .iter()
                .map(|p| format!("{},{}", p.x, p.y))
                .collect::<Vec<_>>()
                .join(" ");
            format!("{}\t{}\t{}\n", one_line(&entry.name), one_line(&entry.description), points)
        })
        .collect()
}

pub fn decode_entries(text: &str) -> Vec<ChooserEntry> {
    text.lines()
        .filter_map(|line| {
            let mut fields = line.splitn(3, '\t');
            Some(ChooserEntry {
                name: fields.next()?.to_string(),
                description: fields.next()?.to_string(),
                points: parse_points(fields.next()?),
            })
        })
        .collect()
}

/// The entry the user picked, from the chooser's output. Nothing means they cancelled.
pub fn parse_choice(output: &str, entry_count: usize) -> Option<usize> {
    output
        .trim()
        .parse::<usize>()
        .ok()
        .filter(|choice| *choice < entry_count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entries_survive_encoding() {
        let entries = vec![
            ChooserEntry {
                name: "Back".to_string(),
                description: "Press keys: Alt+Left".to_string(),
                points: vec![Point2D { x: 100.0, y: 50.5 }, Point2D { x: 0.0, y: 50.0 }],
            },
            ChooserEntry {
                name: "Close".to_string(),
                description: "Close Window".to_string(),
                points: vec![Point2D { x: 0.0, y: 0.0 }, Point2D { x: 9.0, y: 9.0 }],
            },
        ];
        assert_eq!(decode_entries(&encode_entries(&entries)), entries);
    }

    #[test]
    fn separators_in_names_cannot_break_the_format() {
        let entries = vec![ChooserEntry {
            name: "Odd\tname\nhere".to_string(),
            description: String::new(),
            points: Vec::new(),
        }];
        let decoded = decode_entries(&encode_entries(&entries));
        assert_eq!(decoded.len(), 1);
        assert_eq!(decoded[0].name, "Odd name here");
    }

    #[test]
    fn only_a_valid_choice_counts() {
        assert_eq!(parse_choice("1\n", 2), Some(1));
        assert_eq!(parse_choice("2", 2), None);
        assert_eq!(parse_choice("", 2), None);
        assert_eq!(parse_choice("nope", 2), None);
    }
}
