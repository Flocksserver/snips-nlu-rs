use std::collections::HashMap;
use std::ops::Range;

use crate::slot_utils::InternalSlot;
use anyhow::{anyhow, bail, Result};
use snips_nlu_utils::string::suffix_from_char_index;
use snips_nlu_utils::token::Token;

const BEGINNING_PREFIX: &str = "B-";
const INSIDE_PREFIX: &str = "I-";
const LAST_PREFIX: &str = "L-";
const UNIT_PREFIX: &str = "U-";
pub const OUTSIDE: &str = "O";

#[derive(Copy, Clone, Debug)]
pub enum TaggingScheme {
    IO,
    BIO,
    BILOU,
}

impl TaggingScheme {
    pub fn from_u8(i: u8) -> Result<TaggingScheme> {
        match i {
            0 => Ok(TaggingScheme::IO),
            1 => Ok(TaggingScheme::BIO),
            2 => Ok(TaggingScheme::BILOU),
            _ => bail!("Unknown tagging scheme identifier: {}", i),
        }
    }
}

pub fn get_substitution_label<'a>(labels: &[&'a str]) -> &'a str {
    if labels.contains(&OUTSIDE) {
        OUTSIDE
    } else {
        labels[0]
    }
}

pub fn tag_name_to_slot_name(tag: String) -> String {
    suffix_from_char_index(tag, 2)
}

fn is_start_of_io_slot(tags: &[String], i: usize) -> bool {
    if i == 0 {
        tags[i] != OUTSIDE
    } else if tags[i] == OUTSIDE {
        false
    } else {
        tags[i - 1] == OUTSIDE
    }
}

fn is_end_of_io_slot(tags: &[String], i: usize) -> bool {
    if i + 1 == tags.len() {
        tags[i] != OUTSIDE
    } else if tags[i] == OUTSIDE {
        false
    } else {
        tags[i + 1] == OUTSIDE
    }
}

fn is_start_of_bio_slot(tags: &[String], i: usize) -> bool {
    if i == 0 {
        tags[i] != OUTSIDE
    } else if tags[i] == OUTSIDE {
        false
    } else if tags[i].starts_with(BEGINNING_PREFIX) {
        true
    } else {
        tags[i - 1] == OUTSIDE
    }
}

fn is_end_of_bio_slot(tags: &[String], i: usize) -> bool {
    if i + 1 == tags.len() {
        tags[i] != OUTSIDE
    } else {
        tags[i] != OUTSIDE && !tags[i + 1].starts_with(INSIDE_PREFIX)
    }
}

fn is_start_of_bilou_slot(tags: &[String], i: usize) -> bool {
    if i == 0 {
        tags[i] != OUTSIDE
    } else if tags[i] == OUTSIDE {
        false
    } else if tags[i].starts_with(BEGINNING_PREFIX)
        || tags[i].starts_with(UNIT_PREFIX)
        || tags[i - 1].starts_with(UNIT_PREFIX)
        || tags[i - 1].starts_with(LAST_PREFIX)
    {
        true
    } else {
        tags[i - 1] == OUTSIDE
    }
}

fn is_end_of_bilou_slot(tags: &[String], i: usize) -> bool {
    if i + 1 == tags.len() {
        tags[i] != OUTSIDE
    } else if tags[i] == OUTSIDE {
        false
    } else {
        tags[i + 1] == OUTSIDE
            || tags[i].starts_with(LAST_PREFIX)
            || tags[i].starts_with(UNIT_PREFIX)
            || tags[i + 1].starts_with(BEGINNING_PREFIX)
            || tags[i + 1].starts_with(UNIT_PREFIX)
    }
}

pub struct SlotRange {
    slot_name: String,
    pub range: Range<usize>,
    pub char_range: Range<usize>,
}

fn _tags_to_slots<F1, F2>(
    tags: &[String],
    tokens: &[Token],
    is_start_of_slot: F1,
    is_end_of_slot: F2,
) -> Vec<SlotRange>
where
    F1: Fn(&[String], usize) -> bool,
    F2: Fn(&[String], usize) -> bool,
{
    let mut slots: Vec<SlotRange> = Vec::with_capacity(tags.len());

    let mut current_slot_start = 0;
    for (i, tag) in tags.iter().enumerate() {
        if is_start_of_slot(tags, i) {
            current_slot_start = i;
        }
        if is_end_of_slot(tags, i) {
            slots.push(SlotRange {
                range: tokens[current_slot_start].range.start..tokens[i].range.end,
                char_range: tokens[current_slot_start].char_range.start..tokens[i].char_range.end,
                slot_name: tag_name_to_slot_name(tag.to_string()),
            });
            current_slot_start = i;
        }
    }
    slots
}

pub fn tags_to_slot_ranges(
    tokens: &[Token],
    tags: &[String],
    tagging_scheme: TaggingScheme,
) -> Vec<SlotRange> {
    match tagging_scheme {
        TaggingScheme::IO => _tags_to_slots(tags, tokens, is_start_of_io_slot, is_end_of_io_slot),
        TaggingScheme::BIO => {
            _tags_to_slots(tags, tokens, is_start_of_bio_slot, is_end_of_bio_slot)
        }
        TaggingScheme::BILOU => {
            _tags_to_slots(tags, tokens, is_start_of_bilou_slot, is_end_of_bilou_slot)
        }
    }
}

pub fn tags_to_slots(
    text: &str,
    tokens: &[Token],
    tags: &[String],
    tagging_scheme: TaggingScheme,
    intent_slots_mapping: &HashMap<String, String>,
) -> Result<Vec<InternalSlot>> {
    tags_to_slot_ranges(tokens, tags, tagging_scheme)
        .into_iter()
        .map(|s| {
            Ok(InternalSlot {
                value: text[s.range.clone()].to_string(),
                entity: intent_slots_mapping
                    .get(&s.slot_name)
                    .ok_or_else(|| {
                        anyhow!(
                            "Missing slot to entity mapping for slot name: {}",
                            s.slot_name
                        )
                    })?
                    .to_string(),
                char_range: s.char_range,
                slot_name: s.slot_name,
            })
        })
        .collect()
}

pub fn get_scheme_prefix(index: usize, indexes: &[usize], tagging_scheme: TaggingScheme) -> &str {
    match tagging_scheme {
        TaggingScheme::IO => INSIDE_PREFIX,
        TaggingScheme::BIO => {
            if index == indexes[0] {
                BEGINNING_PREFIX
            } else {
                INSIDE_PREFIX
            }
        }
        TaggingScheme::BILOU => {
            if indexes.len() == 1 {
                UNIT_PREFIX
            } else if index == indexes[0] {
                BEGINNING_PREFIX
            } else if index == *indexes.last().unwrap() {
                LAST_PREFIX
            } else {
                INSIDE_PREFIX
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use itertools::Itertools;
    use maplit::hashmap;

    use super::*;
    use snips_nlu_utils::language::Language;
    use snips_nlu_utils::token::tokenize;

    struct Test {
        text: String,
        tags: Vec<String>,
        expected_slots: Vec<InternalSlot>,
    }

    #[test]
    fn test_io_tags_to_slots() {
        // Given
        let language = Language::EN;
        let slot_name = "animal";
        let intent_slots_mapping = hashmap!["animal".to_string() => "animal".to_string()];
        let tags: Vec<Test> = vec![
            Test {
                text: "".to_string(),
                tags: vec![],
                expected_slots: vec![],
            },
            Test {
                text: "nothing here".to_string(),
                tags: vec![OUTSIDE.to_string(), OUTSIDE.to_string()],
                expected_slots: vec![],
            },
            Test {
                text: "i am a blue bird".to_string(),
                tags: vec![
                    OUTSIDE.to_string(),
                    OUTSIDE.to_string(),
                    OUTSIDE.to_string(),
                    format!("{}{}", INSIDE_PREFIX, slot_name),
                    format!("{}{}", INSIDE_PREFIX, slot_name),
                ],
                expected_slots: vec![InternalSlot {
                    char_range: 7..16,
                    value: "blue bird".to_string(),
                    entity: slot_name.to_string(),
                    slot_name: slot_name.to_string(),
                }],
            },
            Test {
                text: "i am a bird".to_string(),
                tags: vec![
                    OUTSIDE.to_string(),
                    OUTSIDE.to_string(),
                    OUTSIDE.to_string(),
                    format!("{}{}", INSIDE_PREFIX, slot_name),
                ],
                expected_slots: vec![InternalSlot {
                    char_range: 7..11,
                    value: "bird".to_string(),
                    entity: slot_name.to_string(),
                    slot_name: slot_name.to_string(),
                }],
            },
            Test {
                text: "bird".to_string(),
                tags: vec![format!("{}{}", INSIDE_PREFIX, slot_name)],
                expected_slots: vec![InternalSlot {
                    char_range: 0..4,
                    value: "bird".to_string(),
                    entity: slot_name.to_string(),
                    slot_name: slot_name.to_string(),
                }],
            },
            Test {
                text: "blue bird".to_string(),
                tags: vec![
                    format!("{}{}", INSIDE_PREFIX, slot_name),
                    format!("{}{}", INSIDE_PREFIX, slot_name),
                ],
                expected_slots: vec![InternalSlot {
                    char_range: 0..9,
                    value: "blue bird".to_string(),
                    entity: slot_name.to_string(),
                    slot_name: slot_name.to_string(),
                }],
            },
            Test {
                text: "light blue bird blue bird".to_string(),
                tags: vec![
                    format!("{}{}", INSIDE_PREFIX, slot_name),
                    format!("{}{}", INSIDE_PREFIX, slot_name),
                    format!("{}{}", INSIDE_PREFIX, slot_name),
                    format!("{}{}", INSIDE_PREFIX, slot_name),
                    format!("{}{}", INSIDE_PREFIX, slot_name),
                ],
                expected_slots: vec![InternalSlot {
                    char_range: 0..25,
                    value: "light blue bird blue bird".to_string(),
                    entity: slot_name.to_string(),
                    slot_name: slot_name.to_string(),
                }],
            },
            Test {
                text: "bird birdy".to_string(),
                tags: vec![
                    format!("{}{}", INSIDE_PREFIX, slot_name),
                    format!("{}{}", INSIDE_PREFIX, slot_name),
                ],
                expected_slots: vec![InternalSlot {
                    char_range: 0..10,
                    value: "bird birdy".to_string(),
                    entity: slot_name.to_string(),
                    slot_name: slot_name.to_string(),
                }],
            },
        ];

        for data in tags {
            // When
            let slots = tags_to_slots(
                &data.text,
                &tokenize(&data.text, language),
                &data.tags,
                TaggingScheme::IO,
                &intent_slots_mapping,
            )
            .unwrap();
            // Then
            assert_eq!(slots, data.expected_slots);
        }
    }

    #[test]
    fn test_bio_tags_to_slots() {
        // Given
        let language = Language::EN;
        let slot_name = "animal";
        let intent_slots_mapping = hashmap!["animal".to_string() => "animal".to_string()];
        let tags: Vec<Test> = vec![
            Test {
                text: "".to_string(),
                tags: vec![],
                expected_slots: vec![],
            },
            Test {
                text: "nothing here".to_string(),
                tags: vec![OUTSIDE.to_string(), OUTSIDE.to_string()],
                expected_slots: vec![],
            },
            Test {
                text: "i am a blue bird".to_string(),
                tags: vec![
                    OUTSIDE.to_string(),
                    OUTSIDE.to_string(),
                    OUTSIDE.to_string(),
                    format!("{}{}", BEGINNING_PREFIX, slot_name),
                    format!("{}{}", INSIDE_PREFIX, slot_name),
                ],
                expected_slots: vec![InternalSlot {
                    char_range: 7..16,
                    value: "blue bird".to_string(),
                    entity: slot_name.to_string(),
                    slot_name: slot_name.to_string(),
                }],
            },
            Test {
                text: "i am a bird".to_string(),
                tags: vec![
                    OUTSIDE.to_string(),
                    OUTSIDE.to_string(),
                    OUTSIDE.to_string(),
                    format!("{}{}", BEGINNING_PREFIX, slot_name),
                ],
                expected_slots: vec![InternalSlot {
                    char_range: 7..11,
                    value: "bird".to_string(),
                    entity: slot_name.to_string(),
                    slot_name: slot_name.to_string(),
                }],
            },
            Test {
                text: "bird".to_string(),
                tags: vec![format!("{}{}", BEGINNING_PREFIX, slot_name)],
                expected_slots: vec![InternalSlot {
                    char_range: 0..4,
                    value: "bird".to_string(),
                    entity: slot_name.to_string(),
                    slot_name: slot_name.to_string(),
                }],
            },
            Test {
                text: "blue bird".to_string(),
                tags: vec![
                    format!("{}{}", BEGINNING_PREFIX, slot_name),
                    format!("{}{}", INSIDE_PREFIX, slot_name),
                ],
                expected_slots: vec![InternalSlot {
                    char_range: 0..9,
                    value: "blue bird".to_string(),
                    entity: slot_name.to_string(),
                    slot_name: slot_name.to_string(),
                }],
            },
            Test {
                text: "light blue bird blue bird".to_string(),
                tags: vec![
                    format!("{}{}", BEGINNING_PREFIX, slot_name),
                    format!("{}{}", INSIDE_PREFIX, slot_name),
                    format!("{}{}", INSIDE_PREFIX, slot_name),
                    format!("{}{}", BEGINNING_PREFIX, slot_name),
                    format!("{}{}", INSIDE_PREFIX, slot_name),
                ],
                expected_slots: vec![
                    InternalSlot {
                        char_range: 0..15,
                        value: "light blue bird".to_string(),
                        entity: slot_name.to_string(),
                        slot_name: slot_name.to_string(),
                    },
                    InternalSlot {
                        char_range: 16..25,
                        value: "blue bird".to_string(),
                        entity: slot_name.to_string(),
                        slot_name: slot_name.to_string(),
                    },
                ],
            },
            Test {
                text: "bird birdy".to_string(),
                tags: vec![
                    format!("{}{}", BEGINNING_PREFIX, slot_name),
                    format!("{}{}", BEGINNING_PREFIX, slot_name),
                ],
                expected_slots: vec![
                    InternalSlot {
                        char_range: 0..4,
                        value: "bird".to_string(),
                        entity: slot_name.to_string(),
                        slot_name: slot_name.to_string(),
                    },
                    InternalSlot {
                        char_range: 5..10,
                        value: "birdy".to_string(),
                        entity: slot_name.to_string(),
                        slot_name: slot_name.to_string(),
                    },
                ],
            },
            Test {
                text: "blue bird and white bird".to_string(),
                tags: vec![
                    format!("{}{}", BEGINNING_PREFIX, slot_name),
                    format!("{}{}", INSIDE_PREFIX, slot_name),
                    OUTSIDE.to_string(),
                    format!("{}{}", INSIDE_PREFIX, slot_name),
                    format!("{}{}", INSIDE_PREFIX, slot_name),
                ],
                expected_slots: vec![
                    InternalSlot {
                        char_range: 0..9,
                        value: "blue bird".to_string(),
                        entity: slot_name.to_string(),
                        slot_name: slot_name.to_string(),
                    },
                    InternalSlot {
                        char_range: 14..24,
                        value: "white bird".to_string(),
                        entity: slot_name.to_string(),
                        slot_name: slot_name.to_string(),
                    },
                ],
            },
        ];

        for data in tags {
            // When
            let slots = tags_to_slots(
                &data.text,
                &tokenize(&data.text, language),
                &data.tags,
                TaggingScheme::BIO,
                &intent_slots_mapping,
            )
            .unwrap();
            // Then
            assert_eq!(slots, data.expected_slots);
        }
    }

    #[test]
    fn test_bilou_tags_to_slots() {
        // Given
        let language = Language::EN;
        let slot_name = "animal";
        let intent_slots_mapping = hashmap!["animal".to_string() => "animal".to_string()];
        let tags: Vec<Test> = vec![
            Test {
                text: "".to_string(),
                tags: vec![],
                expected_slots: vec![],
            },
            Test {
                text: "nothing here".to_string(),
                tags: vec![OUTSIDE.to_string(), OUTSIDE.to_string()],
                expected_slots: vec![],
            },
            Test {
                text: "i am a blue bird".to_string(),
                tags: vec![
                    OUTSIDE.to_string(),
                    OUTSIDE.to_string(),
                    OUTSIDE.to_string(),
                    format!("{}{}", BEGINNING_PREFIX, slot_name),
                    format!("{}{}", LAST_PREFIX, slot_name),
                ],
                expected_slots: vec![InternalSlot {
                    char_range: 7..16,
                    value: "blue bird".to_string(),
                    entity: slot_name.to_string(),
                    slot_name: slot_name.to_string(),
                }],
            },
            Test {
                text: "i am a bird".to_string(),
                tags: vec![
                    OUTSIDE.to_string(),
                    OUTSIDE.to_string(),
                    OUTSIDE.to_string(),
                    format!("{}{}", UNIT_PREFIX, slot_name),
                ],
                expected_slots: vec![InternalSlot {
                    char_range: 7..11,
                    value: "bird".to_string(),
                    entity: slot_name.to_string(),
                    slot_name: slot_name.to_string(),
                }],
            },
            Test {
                text: "bird".to_string(),
                tags: vec![format!("{}{}", UNIT_PREFIX, slot_name)],
                expected_slots: vec![InternalSlot {
                    char_range: 0..4,
                    value: "bird".to_string(),
                    entity: slot_name.to_string(),
                    slot_name: slot_name.to_string(),
                }],
            },
            Test {
                text: "blue bird".to_string(),
                tags: vec![
                    format!("{}{}", BEGINNING_PREFIX, slot_name),
                    format!("{}{}", LAST_PREFIX, slot_name),
                ],
                expected_slots: vec![InternalSlot {
                    char_range: 0..9,
                    value: "blue bird".to_string(),
                    entity: slot_name.to_string(),
                    slot_name: slot_name.to_string(),
                }],
            },
            Test {
                text: "light blue bird blue bird".to_string(),
                tags: vec![
                    format!("{}{}", BEGINNING_PREFIX, slot_name),
                    format!("{}{}", INSIDE_PREFIX, slot_name),
                    format!("{}{}", LAST_PREFIX, slot_name),
                    format!("{}{}", BEGINNING_PREFIX, slot_name),
                    format!("{}{}", LAST_PREFIX, slot_name),
                ],
                expected_slots: vec![
                    InternalSlot {
                        char_range: 0..15,
                        value: "light blue bird".to_string(),
                        entity: slot_name.to_string(),
                        slot_name: slot_name.to_string(),
                    },
                    InternalSlot {
                        char_range: 16..25,
                        value: "blue bird".to_string(),
                        entity: slot_name.to_string(),
                        slot_name: slot_name.to_string(),
                    },
                ],
            },
            Test {
                text: "bird birdy".to_string(),
                tags: vec![
                    format!("{}{}", UNIT_PREFIX, slot_name),
                    format!("{}{}", UNIT_PREFIX, slot_name),
                ],
                expected_slots: vec![
                    InternalSlot {
                        char_range: 0..4,
                        value: "bird".to_string(),
                        entity: slot_name.to_string(),
                        slot_name: slot_name.to_string(),
                    },
                    InternalSlot {
                        char_range: 5..10,
                        value: "birdy".to_string(),
                        entity: slot_name.to_string(),
                        slot_name: slot_name.to_string(),
                    },
                ],
            },
            Test {
                text: "light bird bird blue bird".to_string(),
                tags: vec![
                    format!("{}{}", BEGINNING_PREFIX, slot_name),
                    format!("{}{}", INSIDE_PREFIX, slot_name),
                    format!("{}{}", UNIT_PREFIX, slot_name),
                    format!("{}{}", BEGINNING_PREFIX, slot_name),
                    format!("{}{}", INSIDE_PREFIX, slot_name),
                ],
                expected_slots: vec![
                    InternalSlot {
                        char_range: 0..10,
                        value: "light bird".to_string(),
                        entity: slot_name.to_string(),
                        slot_name: slot_name.to_string(),
                    },
                    InternalSlot {
                        char_range: 11..15,
                        value: "bird".to_string(),
                        entity: slot_name.to_string(),
                        slot_name: slot_name.to_string(),
                    },
                    InternalSlot {
                        char_range: 16..25,
                        value: "blue bird".to_string(),
                        entity: slot_name.to_string(),
                        slot_name: slot_name.to_string(),
                    },
                ],
            },
            Test {
                text: "bird bird bird".to_string(),
                tags: vec![
                    format!("{}{}", LAST_PREFIX, slot_name),
                    format!("{}{}", BEGINNING_PREFIX, slot_name),
                    format!("{}{}", UNIT_PREFIX, slot_name),
                ],
                expected_slots: vec![
                    InternalSlot {
                        char_range: 0..4,
                        value: "bird".to_string(),
                        entity: slot_name.to_string(),
                        slot_name: slot_name.to_string(),
                    },
                    InternalSlot {
                        char_range: 5..9,
                        value: "bird".to_string(),
                        entity: slot_name.to_string(),
                        slot_name: slot_name.to_string(),
                    },
                    InternalSlot {
                        char_range: 10..14,
                        value: "bird".to_string(),
                        entity: slot_name.to_string(),
                        slot_name: slot_name.to_string(),
                    },
                ],
            },
        ];

        for data in tags {
            // When
            let slots = tags_to_slots(
                &data.text,
                &tokenize(&data.text, language),
                &data.tags,
                TaggingScheme::BILOU,
                &intent_slots_mapping,
            )
            .unwrap();
            // Then
            assert_eq!(slots, data.expected_slots);
        }
    }

    #[test]
    fn test_is_start_of_bio_slot() {
        // Given
        let tags = &[
            OUTSIDE.to_string(),
            BEGINNING_PREFIX.to_string(),
            INSIDE_PREFIX.to_string(),
            OUTSIDE.to_string(),
            INSIDE_PREFIX.to_string(),
            OUTSIDE.to_string(),
            BEGINNING_PREFIX.to_string(),
            OUTSIDE.to_string(),
            INSIDE_PREFIX.to_string(),
            BEGINNING_PREFIX.to_string(),
            OUTSIDE.to_string(),
            BEGINNING_PREFIX.to_string(),
            BEGINNING_PREFIX.to_string(),
            INSIDE_PREFIX.to_string(),
            INSIDE_PREFIX.to_string(),
        ];

        // When
        let starts_of_bio = tags
            .iter()
            .enumerate()
            .map(|(i, _)| is_start_of_bio_slot(tags, i))
            .collect_vec();

        // Then
        let expected_starts = [
            false, true, false, false, true, false, true, false, true, true, false, true, true,
            false, false,
        ];

        assert_eq!(starts_of_bio, expected_starts);
    }

    #[test]
    fn test_is_end_of_bio_slot() {
        // Given
        let tags = &[
            OUTSIDE.to_string(),
            BEGINNING_PREFIX.to_string(),
            INSIDE_PREFIX.to_string(),
            OUTSIDE.to_string(),
            INSIDE_PREFIX.to_string(),
            OUTSIDE.to_string(),
            BEGINNING_PREFIX.to_string(),
            OUTSIDE.to_string(),
            INSIDE_PREFIX.to_string(),
            BEGINNING_PREFIX.to_string(),
            OUTSIDE.to_string(),
            BEGINNING_PREFIX.to_string(),
            BEGINNING_PREFIX.to_string(),
            INSIDE_PREFIX.to_string(),
            INSIDE_PREFIX.to_string(),
        ];

        // When
        let ends_of_bio = tags
            .iter()
            .enumerate()
            .map(|(i, _)| is_end_of_bio_slot(tags, i))
            .collect_vec();

        // Then
        let expected_ends = [
            false, false, true, false, true, false, true, false, true, true, false, true, false,
            false, true,
        ];

        assert_eq!(ends_of_bio, expected_ends);
    }

    #[test]
    fn test_start_of_bilou_slot() {
        // Given
        let tags = &[
            OUTSIDE.to_string(),
            LAST_PREFIX.to_string(),
            UNIT_PREFIX.to_string(),
            BEGINNING_PREFIX.to_string(),
            UNIT_PREFIX.to_string(),
            INSIDE_PREFIX.to_string(),
            LAST_PREFIX.to_string(),
            LAST_PREFIX.to_string(),
            UNIT_PREFIX.to_string(),
            UNIT_PREFIX.to_string(),
            LAST_PREFIX.to_string(),
            OUTSIDE.to_string(),
            LAST_PREFIX.to_string(),
            BEGINNING_PREFIX.to_string(),
            INSIDE_PREFIX.to_string(),
            INSIDE_PREFIX.to_string(),
            LAST_PREFIX.to_string(),
        ];

        // When
        let starts_of_bilou = tags
            .iter()
            .enumerate()
            .map(|(i, _)| is_start_of_bilou_slot(tags, i))
            .collect_vec();

        // Then
        let expected_starts = [
            false, true, true, true, true, true, false, true, true, true, true, false, true, true,
            false, false, false,
        ];

        assert_eq!(starts_of_bilou, expected_starts);
    }

    #[test]
    fn test_is_end_of_bilou_slot() {
        // Given
        let tags = &[
            OUTSIDE.to_string(),
            LAST_PREFIX.to_string(),
            UNIT_PREFIX.to_string(),
            BEGINNING_PREFIX.to_string(),
            UNIT_PREFIX.to_string(),
            INSIDE_PREFIX.to_string(),
            LAST_PREFIX.to_string(),
            LAST_PREFIX.to_string(),
            UNIT_PREFIX.to_string(),
            UNIT_PREFIX.to_string(),
            LAST_PREFIX.to_string(),
            OUTSIDE.to_string(),
            INSIDE_PREFIX.to_string(),
            BEGINNING_PREFIX.to_string(),
            OUTSIDE.to_string(),
            BEGINNING_PREFIX.to_string(),
            INSIDE_PREFIX.to_string(),
            INSIDE_PREFIX.to_string(),
            LAST_PREFIX.to_string(),
        ];

        // When
        let ends_of_bilou = tags
            .iter()
            .enumerate()
            .map(|(i, _)| is_end_of_bilou_slot(tags, i))
            .collect_vec();

        // Then
        let expected_ends = [
            false, true, true, true, true, false, true, true, true, true, true, false, true, true,
            false, false, false, false, true,
        ];

        assert_eq!(ends_of_bilou, expected_ends);
    }

    #[test]
    fn tests_get_scheme_prefix() {
        // Given
        let indexes = vec![3, 4, 5];

        // When
        let actual_results = vec![
            get_scheme_prefix(5, &indexes, TaggingScheme::IO).to_string(),
            get_scheme_prefix(3, &indexes, TaggingScheme::BIO).to_string(),
            get_scheme_prefix(4, &indexes, TaggingScheme::BIO).to_string(),
            get_scheme_prefix(3, &indexes, TaggingScheme::BILOU).to_string(),
            get_scheme_prefix(4, &indexes, TaggingScheme::BILOU).to_string(),
            get_scheme_prefix(5, &indexes, TaggingScheme::BILOU).to_string(),
            get_scheme_prefix(1, &[1], TaggingScheme::BILOU).to_string(),
        ];

        // Then
        let expected_results = vec![
            "I-".to_string(),
            "B-".to_string(),
            "I-".to_string(),
            "B-".to_string(),
            "I-".to_string(),
            "L-".to_string(),
            "U-".to_string(),
        ];
        assert_eq!(actual_results, expected_results);
    }
}
