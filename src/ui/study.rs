//! The Study page (Phase 4a): flashcards (spaced repetition), "Quiz me"
//! (AI questions or, offline, fill-in-the-blank from Definition notes), and
//! the focus timer. While a quiz or a review runs, it takes the page.

use eframe::egui::{self, CornerRadius, RichText, Role, Stroke};
use lectio_core::cards::Grade;
use lectio_core::quiz::{Answer, topics};
use lectio_core::{Library, SessionId};

use super::theme::{self, palette, radius, size, space};
use super::{Buffers, View, home, icons, outline, widgets};
use crate::action::{Action, QuizScope, Response};
use crate::app::{QuizRun, ReviewRun};

const WEEK_MS: u64 = 7 * 86_400_000;
const DEFAULT_QUESTIONS: usize = 5;
const DEFAULT_MINUTES: u32 = 25;

pub fn page(ui: &mut egui::Ui, view: &View, buffers: &mut Buffers, actions: &mut Vec<Action>) {
    if let Some(run) = view.study.quiz {
        return quiz_run(ui, view, run, buffers, actions);
    }
    if let Some(run) = view.study.review {
        return review_run(ui, view, run, actions);
    }
    home::page_title(ui, "Study", "Flashcards, quizzes and focused study time", |_| {});
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        card(ui, |ui| flashcards(ui, view, buffers, actions));
        card(ui, |ui| quiz_setup(ui, view, buffers, actions));
        card(ui, |ui| focus(ui, view, buffers, actions));
    });
}

fn card(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui)) {
    widgets::card(ui, add);
    ui.add_space(space::M);
}

fn heading(ui: &mut egui::Ui, icon: &str, title: &str) {
    let p = palette(ui);
    ui.label(widgets::icon_text(icon, title, theme::semibold(size::TITLE), p.text));
    ui.add_space(space::XS);
}

fn muted(ui: &mut egui::Ui, text: &str) {
    let color = palette(ui).text_muted;
    ui.add(egui::Label::new(RichText::new(text).size(size::SMALL).color(color)).wrap());
}

fn flashcards(ui: &mut egui::Ui, view: &View, buffers: &mut Buffers, actions: &mut Vec<Action>) {
    heading(ui, icons::DEFINITION, "Flashcards");
    let total = view.library.cards().len();
    muted(ui, &format!("{} due now · {total} in all. Cards come back just before you would forget them.", view.study.due_cards));
    ui.add_space(space::S);
    ui.horizontal_wrapped(|ui| {
        let label = format!("Review {} due", view.study.due_cards);
        if widgets::primary_button(ui, icons::PLAY, &label, view.study.due_cards > 0).clicked() {
            actions.push(Action::StartReview);
        }
        if widgets::subtle_button(ui, icons::DEFINITION, "Make cards from Definition notes", true).clicked() {
            actions.push(Action::MakeDefinitionCards(None));
        }
    });
    if !view.study.ai_on {
        return;
    }
    ui.add_space(space::S);
    ui.horizontal_wrapped(|ui| {
        session_picker(ui, view.library, &mut buffers.card_session, "card-session");
        let busy = view.study.job.is_some();
        let enabled = buffers.card_session.is_some() && !busy;
        if widgets::subtle_button(ui, icons::TO_FEED, "Write cards with AI", enabled).clicked()
            && let Some(session) = buffers.card_session
        {
            actions.push(Action::GenerateCards(session));
        }
    });
    job_progress(ui, view, "flashcards");
}

fn session_picker(ui: &mut egui::Ui, library: &Library, chosen: &mut Option<SessionId>, id: &str) {
    let sessions = library.live_sessions();
    if chosen.is_none() {
        *chosen = sessions.first().map(|(_, s)| s.id);
    }
    let shown = chosen.and_then(|id| library.session(id)).map_or("Choose a session", |s| s.title.as_str());
    let combo = egui::ComboBox::from_id_salt(id).selected_text(shown).show_ui(ui, |ui| {
        for (course, session) in &sessions {
            ui.selectable_value(chosen, Some(session.id), format!("{} · {}", session.title, course.name));
        }
    });
    widgets::name(&combo.response, Role::ComboBox, "Session");
}

fn job_progress(ui: &mut egui::Ui, view: &View, what: &str) {
    let Some((running, done, total)) = view.study.job.filter(|(w, _, _)| *w == what) else { return };
    let text = format!("Writing the {running}: part {} of {total}…", (done + 1).min(total));
    let bar = ui.add(egui::ProgressBar::new(done as f32 / total.max(1) as f32).text(text));
    widgets::name(&bar, Role::ProgressIndicator, "AI progress");
}

fn scope_label(library: &Library, scope: QuizScope) -> String {
    match scope {
        QuizScope::Session(s) => library.session(s).map_or("?".into(), |s| format!("Session: {}", s.title)),
        QuizScope::Course(c) => library.course(c).map_or("?".into(), |c| format!("Whole course: {}", c.name)),
    }
}

fn quiz_setup(ui: &mut egui::Ui, view: &View, buffers: &mut Buffers, actions: &mut Vec<Action>) {
    heading(ui, icons::REVIEW, "Quiz me");
    let library = view.library;
    if buffers.quiz_scope.is_none() {
        buffers.quiz_scope = library.live_sessions().first().map(|(_, s)| QuizScope::Session(s.id));
        buffers.quiz_count = DEFAULT_QUESTIONS;
        buffers.quiz_use_ai = view.study.ai_on;
    }
    let Some(scope) = buffers.quiz_scope else {
        return muted(ui, "Record or add a session first.");
    };
    ui.horizontal_wrapped(|ui| {
        let combo = egui::ComboBox::from_id_salt("quiz-scope").selected_text(scope_label(library, scope)).show_ui(ui, |ui| {
            for course in library.courses() {
                let whole = QuizScope::Course(course.id);
                if ui.selectable_label(scope == whole, scope_label(library, whole)).clicked() {
                    buffers.quiz_scope = Some(whole);
                    buffers.quiz_topics.clear();
                }
            }
            for (_, session) in library.live_sessions() {
                let one = QuizScope::Session(session.id);
                if ui.selectable_label(scope == one, scope_label(library, one)).clicked() {
                    buffers.quiz_scope = Some(one);
                    buffers.quiz_topics.clear();
                }
            }
        });
        widgets::name(&combo.response, Role::ComboBox, "Quiz on");
        let slider = ui.add(egui::Slider::new(&mut buffers.quiz_count, 3..=20).text("questions"));
        widgets::name(&slider, Role::Slider, "Number of questions");
    });
    topic_choices(ui, library, scope, buffers);
    if view.study.ai_on {
        ui.horizontal(|ui| {
            widgets::switch(ui, &mut buffers.quiz_use_ai, "Questions written by AI");
            ui.label(RichText::new("Questions written by AI (multiple choice and short answer)").size(size::CAPTION));
        });
    }
    if !(view.study.ai_on && buffers.quiz_use_ai) {
        muted(ui, "Offline: fill-in-the-blank questions from your Definition notes (\"Term: meaning\").");
    }
    ui.add_space(space::S);
    let busy = view.study.job.is_some();
    if widgets::primary_button(ui, icons::PLAY, "Start the quiz", !busy).clicked() {
        actions.push(Action::StartQuiz {
            scope,
            count: buffers.quiz_count,
            topics: buffers.quiz_topics.clone(),
            use_ai: buffers.quiz_use_ai && view.study.ai_on,
        });
    }
    job_progress(ui, view, "quiz");
    if let QuizScope::Session(s) = scope
        && let Some(session) = library.session(s)
        && let Some(last) = session.quiz_attempts.last()
    {
        muted(ui, &format!("Last score: {} of {} ({} attempts so far).", last.correct, last.questions, session.quiz_attempts.len()));
    }
}

/// Checkboxes for the headings of the chosen session(s); none = everything.
fn topic_choices(ui: &mut egui::Ui, library: &Library, scope: QuizScope, buffers: &mut Buffers) {
    let sessions: Vec<_> = match scope {
        QuizScope::Session(s) => library.session(s).into_iter().collect(),
        QuizScope::Course(c) => library.live_sessions().into_iter().filter(|(co, _)| co.id == c).map(|(_, s)| s).collect(),
    };
    let titles: Vec<String> = sessions.iter().flat_map(|s| topics(s)).map(|t| t.title).collect();
    if titles.is_empty() {
        return muted(ui, "Topics come from headings; this has none yet, so the quiz covers everything.");
    }
    muted(ui, "Topics (none ticked = everything):");
    ui.horizontal_wrapped(|ui| {
        for title in titles {
            let mut on = buffers.quiz_topics.contains(&title);
            if ui.checkbox(&mut on, &title).changed() {
                if on {
                    buffers.quiz_topics.push(title);
                } else {
                    buffers.quiz_topics.retain(|t| *t != title);
                }
            }
        }
    });
}

fn quiz_run(ui: &mut egui::Ui, view: &View, run: &QuizRun, buffers: &mut Buffers, actions: &mut Vec<Action>) {
    let p = palette(ui);
    let Some(question) = run.questions.get(run.index) else { return };
    let kind = if run.offline { "Fill in the blank" } else { "Quiz" };
    home::page_title(ui, kind, &format!("Question {} of {}", run.index + 1, run.questions.len()), |ui| {
        if widgets::subtle_button(ui, icons::CLOSE, "End the quiz", true).clicked() {
            actions.push(Action::EndQuiz);
        }
    });
    card(ui, |ui| {
        ui.add(egui::Label::new(RichText::new(&question.prompt).font(theme::semibold(size::TITLE)).color(p.text)).wrap());
        ui.add_space(space::M);
        match &question.answer {
            Answer::Choice { choices, correct } => {
                for (index, choice) in choices.iter().enumerate() {
                    let letter = (b'A' + index as u8) as char;
                    let picked = run.given == Some(Response::Choice(index));
                    let fill = match &run.given {
                        Some(_) if index == *correct => p.accent_soft,
                        Some(_) if picked => p.error_fill,
                        _ => p.surface_alt,
                    };
                    let button = egui::Button::new(RichText::new(format!("{letter})  {choice}")).color(p.text))
                        .fill(fill)
                        .min_size(egui::vec2(ui.available_width(), 32.0));
                    let response = ui.add_enabled(run.given.is_none() || picked || index == *correct, button);
                    widgets::name(&response, Role::Button, &format!("Answer {letter}"));
                    if response.clicked() && run.given.is_none() {
                        actions.push(Action::Answer(Response::Choice(index)));
                    }
                }
            }
            Answer::Text { answer } => {
                ui.horizontal(|ui| {
                    let field = ui.add_enabled(
                        run.given.is_none(),
                        egui::TextEdit::singleline(&mut buffers.quiz_text).hint_text("Your answer").desired_width(320.0),
                    );
                    widgets::name(&field, Role::TextInput, "Your answer");
                    let enter = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    if run.given.is_none() && (widgets::primary_button(ui, icons::TASK, "Check", true).clicked() || enter) {
                        actions.push(Action::Answer(Response::Text(std::mem::take(&mut buffers.quiz_text))));
                    }
                });
                if run.given.is_some() {
                    ui.label(RichText::new(format!("Answer: {answer}")).color(p.text));
                }
            }
        }
        if run.given.is_some() {
            ui.add_space(space::S);
            let right = run.results.last().copied().unwrap_or(false);
            let (text, color) = if right { ("Right", p.accent) } else { ("Not quite", p.danger_text) };
            ui.label(RichText::new(text).font(theme::semibold(size::BODY)).color(color));
            if !question.explanation.is_empty() {
                ui.add(egui::Label::new(RichText::new(&question.explanation).color(p.text_muted)).wrap());
            }
            if let Some(session) = view.library.session(question.session) {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new(format!("From {}:", session.title)).size(size::SMALL).color(p.text_muted));
                    outline::citations(ui, session, &question.cites, actions);
                });
            }
        }
        ui.add_space(space::M);
        let last = run.index + 1 == run.questions.len();
        let label = match (run.given.is_some(), last) {
            (true, true) => "Finish",
            (true, false) => "Next question",
            (false, _) => "Skip",
        };
        if widgets::subtle_button(ui, icons::NEXT, label, true).clicked() {
            actions.push(Action::NextQuestion);
        }
    });
}

fn review_run(ui: &mut egui::Ui, view: &View, run: &ReviewRun, actions: &mut Vec<Action>) {
    let p = palette(ui);
    let Some(card_id) = run.queue.get(run.index) else { return };
    let Some(card) = view.library.cards().iter().find(|c| c.id == *card_id) else { return };
    home::page_title(ui, "Flashcards", &format!("Card {} of {}", run.index + 1, run.queue.len()), |ui| {
        if widgets::subtle_button(ui, icons::CLOSE, "Stop reviewing", true).clicked() {
            actions.push(Action::EndReview);
        }
    });
    let frame = egui::Frame::new()
        .fill(p.surface)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(CornerRadius::same(radius::CARD))
        .inner_margin(egui::Margin::same(space::XL as i8));
    frame.show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.vertical_centered(|ui| {
            ui.label(RichText::new(&card.front).font(theme::semibold(size::HEADING)).color(p.text));
            ui.add_space(space::L);
            if run.revealed {
                ui.add(egui::Label::new(RichText::new(&card.back).size(size::TITLE).color(p.text)).wrap());
                if let Some(session) = view.library.session(card.session) {
                    ui.horizontal(|ui| outline::citations(ui, session, &card.cites, actions));
                }
            } else if widgets::primary_button(ui, icons::NEXT, "Show the answer", true).clicked() {
                actions.push(Action::RevealCard);
            }
        });
    });
    if run.revealed {
        ui.add_space(space::M);
        ui.horizontal(|ui| {
            if widgets::icon_button(ui, icons::TRASH, "Delete this card", true).clicked() {
                // Move on first, then delete (grading a deleted card would fail).
                actions.push(Action::GradeCard(Grade::Good));
                actions.push(Action::DeleteCard(card.id));
            }
            muted(ui, "How well did you know it?");
            for (label, grade) in [("Again", Grade::Again), ("Hard", Grade::Hard), ("Good", Grade::Good), ("Easy", Grade::Easy)] {
                let button = ui.button(label);
                widgets::name(&button, Role::Button, label);
                if button.clicked() {
                    actions.push(Action::GradeCard(grade));
                }
            }
        });
    }
}

fn focus(ui: &mut egui::Ui, view: &View, buffers: &mut Buffers, actions: &mut Vec<Action>) {
    let p = palette(ui);
    heading(ui, icons::TASKS, "Focus timer");
    if let Some((course, remaining, running)) = view.study.focus {
        let name = view.library.course(course).map_or("", |c| c.name.as_str());
        let time = lectio_core::timeline::format_offset(remaining);
        ui.label(RichText::new(time).font(theme::mono(size::HEADING * 1.5)).color(p.text));
        muted(ui, &format!("Studying {name}{}", if running { "" } else { " (paused)" }));
        ui.horizontal(|ui| {
            let (label, action) = if running { ("Pause", Action::PauseFocus) } else { ("Resume", Action::ResumeFocus) };
            if widgets::subtle_button(ui, if running { icons::PAUSE } else { icons::PLAY }, label, true).clicked() {
                actions.push(action);
            }
            if widgets::primary_button(ui, icons::TASK, "Finish and log", true).clicked() {
                actions.push(Action::FinishFocus);
            }
            if widgets::subtle_button(ui, icons::CLOSE, "Cancel", true).clicked() {
                actions.push(Action::CancelFocus);
            }
        });
        return;
    }
    let courses = view.library.courses();
    if courses.is_empty() {
        return muted(ui, "Add a course first.");
    }
    if buffers.focus_course.is_none() {
        buffers.focus_course = courses.first().map(|c| c.id);
        buffers.focus_minutes = DEFAULT_MINUTES;
    }
    ui.horizontal_wrapped(|ui| {
        let shown = buffers.focus_course.and_then(|c| view.library.course(c)).map_or("", |c| c.name.as_str());
        let combo = egui::ComboBox::from_id_salt("focus-course").selected_text(shown).show_ui(ui, |ui| {
            for course in courses {
                ui.selectable_value(&mut buffers.focus_course, Some(course.id), &course.name);
            }
        });
        widgets::name(&combo.response, Role::ComboBox, "Focus course");
        let slider = ui.add(egui::Slider::new(&mut buffers.focus_minutes, 5..=120).step_by(5.0).text("minutes"));
        widgets::name(&slider, Role::Slider, "Focus minutes");
        if let Some(course) = buffers.focus_course
            && widgets::primary_button(ui, icons::PLAY, "Start", true).clicked()
        {
            actions.push(Action::StartFocus { course, minutes: buffers.focus_minutes });
        }
    });
    let week: Vec<(String, u32)> = courses
        .iter()
        .map(|c| (c.name.clone(), view.library.focus_minutes(c.id, view.now_ms.saturating_sub(WEEK_MS))))
        .filter(|(_, m)| *m > 0)
        .collect();
    if !week.is_empty() {
        let summary: Vec<String> = week.iter().map(|(name, m)| format!("{name} {m} min")).collect();
        muted(ui, &format!("This week: {}", summary.join(" · ")));
    }
}

/// "Focus 18:20" chip for the toolbars while a timer runs.
pub fn focus_chip(ui: &mut egui::Ui, view: &View, actions: &mut Vec<Action>) {
    let Some((_, remaining, running)) = view.study.focus else { return };
    let text = format!("Focus {}{}", lectio_core::timeline::format_offset(remaining), if running { "" } else { " ⏸" });
    let chip = widgets::neutral_chip(ui, Some(icons::TASKS), &text, true, "Open the focus timer");
    if chip.clicked() {
        actions.push(Action::GoTo(crate::action::Screen::Study));
    }
}
