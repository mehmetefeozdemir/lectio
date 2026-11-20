# Fonts bundled with Lectio

| File | Font | Source | Licence |
|---|---|---|---|
| `Inter-Regular.ttf`, `Inter-SemiBold.ttf` | Inter 4.1 (static TTFs from `extras/ttf/`) | github.com/rsms/inter, release v4.1 | SIL Open Font License 1.1 (`Inter-OFL.txt`) |
| `JetBrainsMono-Regular.ttf` | JetBrains Mono 2.304 | github.com/JetBrains/JetBrainsMono, release v2.304 | SIL Open Font License 1.1 (`JetBrainsMono-OFL.txt`) |
| `NotoSansMath-Regular.ttf` | Noto Sans Math 3.000 (formulas, ADR-023) | github.com/google/fonts, `ofl/notosansmath/`, commit dbd1ab6 (SHA-256 3f495fe9…2c47d) | SIL Open Font License 1.1 (`NotoSansMath-OFL.txt`) |
| `Phosphor.ttf` | Phosphor Icons, regular weight | the `res/` folder of the `egui-phosphor` 0.14.0 crate (crates.io) | MIT (Phosphor Icons; packaging: `Phosphor-egui-phosphor-MIT.txt`) |

The fonts are embedded with `include_bytes!` in `src/ui/theme.rs` (ADR-014). Codepoints for the icons are in
`src/ui/icons.rs`.
