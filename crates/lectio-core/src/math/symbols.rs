//! The LaTeX commands the formula parser knows, and the mapping from plain
//! letters to Unicode's mathematical alphabets (ADR-023).

use super::Class;

/// A command that stands for one symbol, and how it spaces itself.
pub(super) fn symbol(name: &str) -> Option<(char, Class)> {
    let greek_lower = [
        ("alpha", 'α'), ("beta", 'β'), ("gamma", 'γ'), ("delta", 'δ'), ("epsilon", 'ϵ'), ("varepsilon", 'ε'),
        ("zeta", 'ζ'), ("eta", 'η'), ("theta", 'θ'), ("vartheta", 'ϑ'), ("iota", 'ι'), ("kappa", 'κ'),
        ("lambda", 'λ'), ("mu", 'μ'), ("nu", 'ν'), ("xi", 'ξ'), ("pi", 'π'), ("varpi", 'ϖ'), ("rho", 'ρ'),
        ("varrho", 'ϱ'), ("sigma", 'σ'), ("varsigma", 'ς'), ("tau", 'τ'), ("upsilon", 'υ'), ("phi", 'ϕ'),
        ("varphi", 'φ'), ("chi", 'χ'), ("psi", 'ψ'), ("omega", 'ω'),
    ];
    if let Some((_, ch)) = greek_lower.iter().find(|(n, _)| *n == name) {
        return Some((italic(*ch), Class::Ord));
    }
    let table: &[(&str, char, Class)] = &[
        // Capital Greek letters are upright.
        ("Gamma", 'Γ', Class::Ord), ("Delta", 'Δ', Class::Ord), ("Theta", 'Θ', Class::Ord), ("Lambda", 'Λ', Class::Ord),
        ("Xi", 'Ξ', Class::Ord), ("Pi", 'Π', Class::Ord), ("Sigma", 'Σ', Class::Ord), ("Upsilon", 'Υ', Class::Ord),
        ("Phi", 'Φ', Class::Ord), ("Psi", 'Ψ', Class::Ord), ("Omega", 'Ω', Class::Ord),
        // Binary operators.
        ("pm", '±', Class::Bin), ("mp", '∓', Class::Bin), ("times", '×', Class::Bin), ("cdot", '⋅', Class::Bin),
        ("div", '÷', Class::Bin), ("ast", '∗', Class::Bin), ("circ", '∘', Class::Bin), ("cup", '∪', Class::Bin),
        ("cap", '∩', Class::Bin), ("setminus", '∖', Class::Bin), ("oplus", '⊕', Class::Bin), ("otimes", '⊗', Class::Bin),
        ("land", '∧', Class::Bin), ("wedge", '∧', Class::Bin), ("lor", '∨', Class::Bin), ("vee", '∨', Class::Bin),
        // Relations and arrows.
        ("le", '≤', Class::Rel), ("leq", '≤', Class::Rel), ("ge", '≥', Class::Rel), ("geq", '≥', Class::Rel),
        ("ne", '≠', Class::Rel), ("neq", '≠', Class::Rel), ("approx", '≈', Class::Rel), ("equiv", '≡', Class::Rel),
        ("sim", '∼', Class::Rel), ("simeq", '≃', Class::Rel), ("cong", '≅', Class::Rel), ("propto", '∝', Class::Rel),
        ("in", '∈', Class::Rel), ("notin", '∉', Class::Rel), ("ni", '∋', Class::Rel), ("subset", '⊂', Class::Rel),
        ("subseteq", '⊆', Class::Rel), ("supset", '⊃', Class::Rel), ("supseteq", '⊇', Class::Rel),
        ("to", '→', Class::Rel), ("rightarrow", '→', Class::Rel), ("leftarrow", '←', Class::Rel), ("gets", '←', Class::Rel),
        ("leftrightarrow", '↔', Class::Rel), ("Rightarrow", '⇒', Class::Rel), ("Leftarrow", '⇐', Class::Rel),
        ("Leftrightarrow", '⇔', Class::Rel), ("implies", '⟹', Class::Rel), ("iff", '⟺', Class::Rel),
        ("mapsto", '↦', Class::Rel), ("mid", '∣', Class::Rel), ("parallel", '∥', Class::Rel), ("perp", '⊥', Class::Rel),
        ("ll", '≪', Class::Rel), ("gg", '≫', Class::Rel),
        // Ordinary symbols.
        ("infty", '∞', Class::Ord), ("partial", '∂', Class::Ord), ("nabla", '∇', Class::Ord), ("forall", '∀', Class::Ord),
        ("exists", '∃', Class::Ord), ("nexists", '∄', Class::Ord), ("emptyset", '∅', Class::Ord), ("varnothing", '∅', Class::Ord),
        ("neg", '¬', Class::Ord), ("lnot", '¬', Class::Ord), ("angle", '∠', Class::Ord), ("triangle", '△', Class::Ord),
        ("prime", '′', Class::Ord), ("ldots", '…', Class::Ord), ("dots", '…', Class::Ord), ("cdots", '⋯', Class::Ord),
        ("vdots", '⋮', Class::Ord), ("ddots", '⋱', Class::Ord), ("ell", 'ℓ', Class::Ord), ("hbar", 'ℏ', Class::Ord),
        ("Re", 'ℜ', Class::Ord), ("Im", 'ℑ', Class::Ord), ("aleph", 'ℵ', Class::Ord), ("top", '⊤', Class::Ord),
        ("bot", '⊥', Class::Ord), ("degree", '°', Class::Ord),
        // Delimiters.
        ("langle", '⟨', Class::Open), ("rangle", '⟩', Class::Close), ("lfloor", '⌊', Class::Open), ("rfloor", '⌋', Class::Close),
        ("lceil", '⌈', Class::Open), ("rceil", '⌉', Class::Close), ("lvert", '|', Class::Open), ("rvert", '|', Class::Close),
        ("lVert", '‖', Class::Open), ("rVert", '‖', Class::Close), ("|", '‖', Class::Ord),
        ("{", '{', Class::Open), ("}", '}', Class::Close),
        // Escaped characters.
        ("%", '%', Class::Ord), ("$", '$', Class::Ord), ("#", '#', Class::Ord), ("&", '&', Class::Ord), ("_", '_', Class::Ord),
        // Big operators: limits go to the side (integrals) or above and below (the rest).
        ("int", '∫', Class::Op { limits: false }), ("iint", '∬', Class::Op { limits: false }),
        ("iiint", '∭', Class::Op { limits: false }), ("oint", '∮', Class::Op { limits: false }),
        ("sum", '∑', Class::Op { limits: true }), ("prod", '∏', Class::Op { limits: true }),
        ("coprod", '∐', Class::Op { limits: true }), ("bigcup", '⋃', Class::Op { limits: true }),
        ("bigcap", '⋂', Class::Op { limits: true }),
    ];
    table.iter().find(|(n, _, _)| *n == name).map(|(_, ch, class)| (*ch, *class))
}

/// Named functions, written upright: `Some(limits go above and below)`.
pub(super) fn function(name: &str) -> Option<bool> {
    const SIDE: [&str; 22] = [
        "sin", "cos", "tan", "cot", "sec", "csc", "arcsin", "arccos", "arctan", "sinh", "cosh", "tanh", "log", "ln", "lg",
        "exp", "det", "dim", "ker", "gcd", "deg", "arg",
    ];
    const ABOVE: [&str; 7] = ["lim", "liminf", "limsup", "max", "min", "sup", "inf"];
    if SIDE.contains(&name) {
        Some(false)
    } else if ABOVE.contains(&name) {
        Some(true)
    } else {
        None
    }
}

/// Space commands, in ems.
pub(super) fn space(name: &str) -> Option<f32> {
    Some(match name {
        "," => 3.0 / 18.0,
        ":" | ">" => 4.0 / 18.0,
        ";" => 5.0 / 18.0,
        "!" => -3.0 / 18.0,
        " " => 1.0 / 3.0,
        "quad" => 1.0,
        "qquad" => 2.0,
        _ => return None,
    })
}

fn offset(ch: char, from: char, to: u32) -> char {
    char::from_u32(to + (ch as u32 - from as u32)).unwrap_or(ch)
}

/// Math italic: `x` → `𝑥`, `α` → `𝛼` (the usual look of variables).
pub(super) fn italic(ch: char) -> char {
    match ch {
        'h' => 'ℎ', // the one letter Unicode placed elsewhere
        'a'..='z' => offset(ch, 'a', 0x1D44E),
        'A'..='Z' => offset(ch, 'A', 0x1D434),
        'α'..='ω' => offset(ch, 'α', 0x1D6FC),
        'ϵ' => '𝜖',
        'ϑ' => '𝜗',
        'ϕ' => '𝜙',
        'ϱ' => '𝜚',
        'ϖ' => '𝜛',
        _ => ch,
    }
}

/// Bold (`\mathbf`): letters and digits.
pub(super) fn bold(ch: char) -> char {
    match ch {
        'a'..='z' => offset(ch, 'a', 0x1D41A),
        'A'..='Z' => offset(ch, 'A', 0x1D400),
        '0'..='9' => offset(ch, '0', 0x1D7CE),
        _ => ch,
    }
}

/// Double-struck (`\mathbb`): ℝ, ℕ, ℤ, ℚ, ℂ and the rest.
pub(super) fn double_struck(ch: char) -> char {
    match ch {
        'C' => 'ℂ',
        'H' => 'ℍ',
        'N' => 'ℕ',
        'P' => 'ℙ',
        'Q' => 'ℚ',
        'R' => 'ℝ',
        'Z' => 'ℤ',
        'A'..='Z' => offset(ch, 'A', 0x1D538),
        'a'..='z' => offset(ch, 'a', 0x1D552),
        '0'..='9' => offset(ch, '0', 0x1D7D8),
        _ => ch,
    }
}
