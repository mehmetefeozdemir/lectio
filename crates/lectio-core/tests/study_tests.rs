//! Phase 4a: material for AI study tools (sources, chunking under a token
//! budget, citations), the outline, and quizzes (parsing, offline cloze,
//! grading, topics).

use lectio_core::material::{BlobKey, MediaKind, SlideLink};
use lectio_core::outline::{Outline, merge_outlines, parse_outline};
use lectio_core::quiz::{
    Answer, QuizAttempt, cloze_questions, grade_text, parse_questions, sources_in_topics, split_definition, spread, topics,
};
use lectio_core::sources::{
    Citation, Source, SourceKind, chunk, estimate_tokens, gather, resolve, sample_chunks, split_citations,
};
use lectio_core::transcript::{Segment, Transcript};
use lectio_core::{LabelKind, Library, Recording, SessionId};

fn lecture() -> (Library, SessionId) {
    let (lib, course) = Library::default().with_course("CSCA67").unwrap();
    let (lib, s) = lib.with_session(course, "Graphs", 0).unwrap();
    let recording = Recording { file_name: "g.wav".into(), duration_ms: 600_000, sample_rate: 16_000, cleaned_file: None };
    let lib = lib.with_recording(s, recording).unwrap();
    let (lib, _) = lib.with_note(s, 10_000, "Breadth-first search", &[LabelKind::Heading]).unwrap();
    let (lib, _) = lib.with_note(s, 20_000, "BFS: visits vertices in order of distance", &[LabelKind::Definition]).unwrap();
    let (lib, deck) = lib.with_deck(s, "W3.pdf", BlobKey::new_random(MediaKind::Pdf), 2).unwrap();
    let lib = lib.with_slide_text(s, deck, vec!["Graphs".into(), "Queue, distance order".into()]).unwrap();
    let (lib, _) = lib.with_slide_item(s, 30_000, SlideLink { deck, page: 2 }).unwrap();
    let (lib, _) = lib.with_note(s, 300_000, "Depth-first search", &[LabelKind::Heading]).unwrap();
    let (lib, _) =
        lib.with_note(s, 310_000, "DFS: explores as far as possible before backtracking", &[LabelKind::Definition]).unwrap();
    let segments = vec![
        Segment { start_ms: 40_000, end_ms: 45_000, text: "We keep a queue of vertices to visit.".into() },
        Segment { start_ms: 320_000, end_ms: 330_000, text: "A stack gives depth-first order.".into() },
    ];
    let transcript = Transcript { language: "en".into(), model: "tiny".into(), segments, complete: true };
    (lib.with_transcript(s, transcript).unwrap(), s)
}

fn source(id: u32, offset_ms: u64, text: &str) -> Source {
    Source { id, kind: SourceKind::Note, offset_ms: Some(offset_ms), slide: None, text: text.to_owned() }
}

#[test]
fn sources_are_numbered_in_time_order_with_slide_text_after_its_slide() {
    let (lib, s) = lecture();
    let sources = gather(lib.session(s).unwrap());

    let ids: Vec<u32> = sources.iter().map(|x| x.id).collect();
    assert_eq!(ids, (1..=sources.len() as u32).collect::<Vec<_>>());
    let offsets: Vec<u64> = sources.iter().filter_map(|x| x.offset_ms).collect();
    assert!(offsets.windows(2).all(|w| w[0] <= w[1]), "time order: {offsets:?}");
    let slide_text = sources.iter().find(|x| x.kind == SourceKind::SlideText).unwrap();
    assert_eq!(slide_text.text, "Queue, distance order");
    assert_eq!(slide_text.offset_ms, Some(30_000), "the posted slide's moment");
    assert!(sources.iter().any(|x| x.kind == SourceKind::Transcript && x.offset_ms == Some(40_000)));
    assert!(sources.iter().any(|x| x.kind == SourceKind::Heading));
}

#[test]
fn a_cited_line_that_shares_too_little_with_the_item_is_taken_as_a_wrong_number() {
    let sources = vec![
        source(1, 5_000, "Breadth-first search visits a graph level by level"),
        source(2, 11_000, "Queue: a list where the first item added is the first removed"),
        source(3, 18_000, "Depth-first search follows one path as far as it goes"),
        source(4, 21_000, "Stack: a list where the last item added is the first removed"),
    ];
    // What the 0.5B model wrote: its headings numbered 1, 2, 3… instead of cited.
    let reply = "[S1] (0:05) Breadth-first search basics\n[Sources: S1, S4]\n\n\
                 [S2] (0:11) Depth-first search details\n[Sources: S2, S4]\n\n\
                 [S4] (0:21) Planets near the Sun";

    let items = parse_outline(reply, &sources);

    let starts: Vec<(&str, Option<u64>)> = items.iter().map(|i| (i.title.as_str(), i.start_ms())).collect();
    assert_eq!(
        starts,
        vec![
            ("Breadth-first search basics", Some(5_000)),
            ("Depth-first search details", Some(18_000)), // S2 only shares "first"; S3 shares three words
            ("Planets near the Sun", Some(21_000)),        // nothing fits better: the model's number stands
        ]
    );
}

#[test]
fn doubled_list_markers_are_not_part_of_a_title() {
    let sources = vec![source(1, 5_000, "Breadth-first search visits a graph level by level")];

    let items = parse_outline("- - Breadth-first search basics [S1]\n1. - Breadth-first search levels [S1]\n* • BFS levels [S1]", &sources);

    let titles: Vec<&str> = items.iter().map(|i| i.title.as_str()).collect();
    assert_eq!(titles, vec!["Breadth-first search basics", "Breadth-first search levels", "BFS levels"]);
}

#[test]
fn a_handful_of_short_notes_is_too_little_for_an_outline() {
    let six_notes: Vec<Source> = (1..=6).map(|i| source(i, u64::from(i) * 1_000, "Stack: last item added is first removed")).collect();
    let lecture_minutes: Vec<Source> = (1..=30)
        .map(|i| source(i, u64::from(i) * 10_000, "We keep a queue of the vertices still to visit, closest first."))
        .collect();

    assert!(!lectio_core::prompts::enough_for_outline(&six_notes), "the model can only repeat them");
    assert!(lectio_core::prompts::enough_for_outline(&lecture_minutes));
    assert!(!lectio_core::prompts::enough_for_outline(&[]));
}

#[test]
fn tokens_are_estimated_at_four_characters_each() {
    assert_eq!(estimate_tokens(""), 0);
    assert_eq!(estimate_tokens("abcd"), 1);
    assert_eq!(estimate_tokens("abcde"), 2);
}

#[test]
fn chunks_stay_under_the_budget_and_keep_order() {
    let sources: Vec<Source> = (1..=40).map(|i| source(i, u64::from(i) * 1_000, "word ".repeat(10).trim())).collect();

    let chunks = chunk(&sources, 100);

    assert!(chunks.len() > 1);
    assert!(chunks.iter().all(|c| c.tokens <= 100 && estimate_tokens(&c.text) <= 100));
    let ids: Vec<u32> = chunks.iter().flat_map(|c| c.sources.iter().map(|x| x.id)).collect();
    assert_eq!(ids, (1..=40).collect::<Vec<_>>(), "every source once, in order");
    assert!(chunks[0].text.starts_with("[S1] (0:01) word"));
}

#[test]
fn an_over_long_source_is_cut_at_a_word_and_marked() {
    let long = "lorem ipsum ".repeat(200);
    let chunks = chunk(&[source(1, 0, &long)], 50);

    assert_eq!(chunks.len(), 1);
    assert!(chunks[0].truncated);
    assert!(chunks[0].tokens <= 50);
    assert!(chunks[0].text.trim_end().ends_with('…'));
    assert!(!chunks[0].text.contains("ipsu…") && !chunks[0].text.contains("lore…"), "cut at a word");
}

#[test]
fn nothing_to_chunk_gives_no_chunks() {
    assert!(chunk(&[], 100).is_empty());
}

#[test]
fn a_collection_keeps_an_even_sample_of_chunks() {
    let sources: Vec<Source> = (1..=50).map(|i| source(i, u64::from(i), &"x".repeat(80))).collect();
    let chunks = chunk(&sources, 25);
    assert_eq!(chunks.len(), 50);

    let kept = sample_chunks(chunks, 5);

    let first_ids: Vec<u32> = kept.iter().map(|c| c.sources[0].id).collect();
    assert_eq!(first_ids, vec![1, 11, 21, 31, 41]);
}

#[test]
fn source_markers_are_split_off_and_mapped_to_moments() {
    let (text, ids) = split_citations("BFS uses a queue [S3, S5] [S9]");
    assert_eq!(text, "BFS uses a queue");
    assert_eq!(ids, vec![3, 5, 9]);
    assert_eq!(split_citations("Sources: S2,S4").1, vec![2, 4]);

    let link = SlideLink { deck: lectio_core::DeckId::new_random(), page: 2 };
    let sources = vec![
        source(3, 17_000, "a"),
        Source { id: 5, kind: SourceKind::SlideText, offset_ms: None, slide: Some(link), text: "b".into() },
    ];
    let cites = resolve(&[3, 5, 99, 3], &sources);
    let expected = vec![Citation { offset_ms: Some(17_000), slide: None }, Citation { offset_ms: None, slide: Some(link) }];
    assert_eq!(cites, expected);
}

#[test]
fn outline_lines_need_a_valid_source() {
    let sources = vec![source(1, 10_000, "a"), source(2, 300_000, "b")];
    let reply = "Here is the outline:\n- [S1] Breadth-first search\n  - Queue order [S1]\n- Made-up section\n\
                 ## Depth-first search [S2]\n- Ghost [S77]";

    let items = parse_outline(reply, &sources);

    let titles: Vec<(&str, u8)> = items.iter().map(|i| (i.title.as_str(), i.level)).collect();
    assert_eq!(titles, vec![("Breadth-first search", 1), ("Queue order", 2), ("Depth-first search", 1)]);
    assert_eq!(items[2].start_ms(), Some(300_000));
}

#[test]
fn outline_parts_merge_in_time_order_without_duplicates() {
    let sources = vec![source(1, 10_000, "a"), source(2, 300_000, "b")];
    let late = parse_outline("- [S2] DFS", &sources);
    let early = parse_outline("- [S1] BFS\n- [S1] bfs", &sources);

    let merged = merge_outlines(vec![late, early]);

    let titles: Vec<&str> = merged.iter().map(|i| i.title.as_str()).collect();
    assert_eq!(titles, vec!["BFS", "DFS"]);
}

#[test]
fn an_outline_is_stored_and_can_become_heading_notes() {
    let (lib, s) = lecture();
    let sources = gather(lib.session(s).unwrap());
    let queue = sources.iter().find(|x| x.text.starts_with("Queue")).unwrap().id;
    let items = parse_outline(&format!("- [S1] Graphs intro\n- [S{queue}] Queue and distance order"), &sources);
    let outline = Outline { items, model: "test".into(), created_unix_ms: 1 };

    let lib = lib.with_outline(s, outline).unwrap();
    let (lib, added) = lib.with_outline_headings(s).unwrap();

    assert_eq!(added, 2);
    let headings: Vec<&str> =
        lib.session(s).unwrap().notes.iter().filter(|n| n.is_heading()).map(|n| n.body.as_str()).collect();
    assert!(headings.contains(&"Graphs intro"));
    let (_, again) = lib.with_outline_headings(s).unwrap();
    assert_eq!(again, 0, "not added twice");
}

#[test]
fn quiz_blocks_are_parsed_and_broken_ones_dropped() {
    let (_, s) = lecture();
    let sources = vec![source(1, 10_000, "a"), source(2, 20_000, "b")];
    let reply = "Q: Which structure does BFS use?\nA) Stack\nB) Queue\nC) Heap\nD) Tree\nAnswer: B\nWhy: It visits by distance.\nSources: S1\n\n\
                 Q: Name the search that backtracks.\nAnswer: Depth-first search\nWhy: It goes deep first.\nSources: S2\n\n\
                 Q: Broken, answer out of range\nA) x\nB) y\nAnswer: E\nSources: S1\n\n\
                 Q: No sources\nAnswer: x\nWhy: y";

    let questions = parse_questions(reply, &sources, s);

    assert_eq!(questions.len(), 2);
    let choices = vec!["Stack".into(), "Queue".into(), "Heap".into(), "Tree".into()];
    assert_eq!(questions[0].answer, Answer::Choice { choices, correct: 1 });
    assert_eq!(questions[0].cites, vec![Citation { offset_ms: Some(10_000), slide: None }]);
    assert_eq!(questions[1].answer, Answer::Text { answer: "Depth-first search".into() });
    assert_eq!(questions[1].explanation, "It goes deep first.");
}

#[test]
fn definition_notes_split_into_term_and_meaning() {
    assert_eq!(split_definition("BFS: visits by distance"), Some(("BFS".into(), "visits by distance".into())));
    assert_eq!(split_definition("Graph — vertices and edges"), Some(("Graph".into(), "vertices and edges".into())));
    assert_eq!(split_definition("Tree - a connected acyclic graph"), Some(("Tree".into(), "a connected acyclic graph".into())));
    assert_eq!(split_definition("just a sentence without a term"), None);
}

#[test]
fn the_offline_quiz_blanks_out_defined_terms() {
    let (lib, s) = lecture();

    let questions = cloze_questions(&lib, &[s]);

    assert_eq!(questions.len(), 2);
    assert_eq!(questions[0].prompt, "_____: visits vertices in order of distance");
    assert_eq!(questions[0].answer, Answer::Text { answer: "BFS".into() });
    assert_eq!(questions[0].cites[0].offset_ms, Some(20_000));
}

#[test]
fn short_answers_are_graded_leniently() {
    assert!(grade_text("depth first search", "Depth-first search"));
    assert!(grade_text(" BFS ", "bfs"));
    assert!(grade_text("it is a Queue", "queue"));
    assert!(!grade_text("stack", "queue"));
    assert!(!grade_text("", "queue"));
}

#[test]
fn topics_come_from_headings_and_select_their_sources() {
    let (lib, s) = lecture();
    let session = lib.session(s).unwrap();

    let found = topics(session);

    let titles: Vec<&str> = found.iter().map(|t| t.title.as_str()).collect();
    assert_eq!(titles, vec!["Breadth-first search", "Depth-first search"]);
    assert_eq!((found[0].start_ms, found[0].end_ms), (10_000, 300_000));
    let dfs_only = sources_in_topics(&gather(session), &found[1..]);
    assert!(dfs_only.iter().all(|x| x.offset_ms.is_some_and(|o| o >= 300_000)));
    assert!(dfs_only.iter().any(|x| x.text.contains("stack")));
}

#[test]
fn questions_are_spread_evenly_over_chunks() {
    assert_eq!(spread(10, 3), vec![4, 3, 3]);
    assert_eq!(spread(2, 4), vec![1, 1, 0, 0]);
    assert!(spread(5, 0).is_empty());
}

#[test]
fn quiz_attempts_are_kept_per_session() {
    let (lib, s) = lecture();
    let attempt = QuizAttempt { unix_ms: 5, questions: 4, correct: 3, topics: vec!["BFS".into()] };

    let lib = lib.with_quiz_attempt(s, attempt.clone()).unwrap();

    assert_eq!(lib.session(s).unwrap().quiz_attempts, vec![attempt]);
    let bad = QuizAttempt { unix_ms: 6, questions: 2, correct: 3, topics: vec![] };
    assert!(lib.with_quiz_attempt(s, bad).is_err(), "more correct than asked");
}

#[test]
fn generated_flashcards_are_parsed_with_their_sources() {
    let sources = vec![source(1, 10_000, "a"), source(2, 20_000, "b")];
    let reply = "Front: BFS\nBack: Visits vertices by distance\nSources: S1\n\nFront: Stack\nBack: LIFO\n\nFront: DFS\nBack: Goes deep first [S2]";

    let cards = lectio_core::cards::parse_cards(reply, &sources);

    assert_eq!(cards.len(), 2, "the card without a source is dropped");
    assert_eq!(cards[0], ("BFS".to_owned(), "Visits vertices by distance".to_owned(), vec![Citation { offset_ms: Some(10_000), slide: None }]));
    assert_eq!(cards[1].1, "Goes deep first");
}

#[test]
fn prompts_carry_the_numbered_material_and_the_requested_count() {
    let sources = vec![source(1, 10_000, "BFS uses a queue")];
    let chunks = chunk(&sources, 500);

    let (system, user) = lectio_core::prompts::quiz(&chunks[0], 3, &["BFS".to_owned()]);

    assert!(system.contains("Sources:"));
    assert!(user.contains("[S1] (0:10) BFS uses a queue"));
    assert!(user.contains("3 questions"));
    assert!(user.contains("BFS"));
    let (_, outline_user) = lectio_core::prompts::outline(&chunks[0]);
    assert!(outline_user.contains("[S1]"));
}

#[test]
fn questions_with_placeholder_or_repeated_choices_are_dropped() {
    let (_, s) = lecture();
    let sources = vec![source(1, 10_000, "a")];
    let reply = "Q: What is BFS?\nA) Choice\nB) Choice\nC) Choice\nD) Choice\nAnswer: D\nWhy: x\nSources: S1\n\n\
                 Q: question\nAnswer: short answer\nWhy: one sentence\nSources: S1";

    assert!(parse_questions(reply, &sources, s).is_empty());
}

#[test]
fn outline_titles_that_copy_a_whole_source_line_are_dropped() {
    let sources = vec![source(1, 10_000, "We start at one vertex and visit all of its neighbours first."), source(2, 20_000, "BFS")];
    let reply = "[S1] We start at one vertex and visit all of its neighbours first.\n[S2] Breadth-first search";

    let titles: Vec<String> = parse_outline(reply, &sources).into_iter().map(|i| i.title).collect();

    assert_eq!(titles, vec!["Breadth-first search"]);
}

#[test]
fn an_item_without_source_numbers_is_cited_to_the_line_it_matches() {
    let sources = vec![
        source(1, 10_000, "BFS visits vertices in order of distance from the start vertex"),
        source(2, 20_000, "DFS uses a stack or recursion to go deep first"),
    ];
    assert_eq!(lectio_core::sources::best_match("Which structure does DFS use? A stack", &sources), Some(2));
    assert_eq!(lectio_core::sources::best_match("What is photosynthesis?", &sources), None);

    let (_, s) = lecture();
    let reply = "Q: In what order does BFS visit vertices?\nAnswer: By distance from the start\nWhy: It explores level by level.";
    let questions = parse_questions(reply, &sources, s);
    assert_eq!(questions.len(), 1);
    assert_eq!(questions[0].cites[0].offset_ms, Some(10_000));
}

#[test]
fn outline_lines_that_echo_the_material_with_its_time_are_dropped() {
    let sources = vec![source(11, 1_024_000, "That means we visit them in order of distance from the start.")];
    let reply = "[S11] (17:04) That means we visit them in order of distance from the start.\n- Visiting order [S11]";

    let titles: Vec<String> = parse_outline(reply, &sources).into_iter().map(|i| i.title).collect();

    assert_eq!(titles, vec!["Visiting order"]);
}
