# Contributing to Griffel

Thank you for considering a contribution. Griffel is developed **clean-room**
from a behavioral specification in `docs/`, and that process is the project's
most important legal and ethical property. One rule protects it:

## The clean-room declaration (required)

Before opening a pull request or an issue, you must be able to truthfully
declare the following:

> **I declare that I have never read, studied, decompiled, or otherwise
> accessed the source code, assets, or any other implementation material of
> [Apostrophe](https://gitlab.gnome.org/World/apostrophe), and that my
> contribution contains nothing derived from them.**

- **Pull requests** must include this declaration — the PR template has a
  required checkbox for it. If you *have* read Apostrophe's implementation in
  the past, please describe what and how thoroughly; the maintainers will
  decide whether your contribution can still be accepted. An honest
  disclosure is always welcome; an undeclared one is not.
- **Issues** (bug reports, feature requests) also require the declaration via
  the issue form. Even describing behavior from memory of Apostrophe's source
  can taint the clean-room process.
- Public documentation, screenshots, release notes, and the app itself do not
  count as "implementation material" — comparing observable behavior with
  what Griffel already does is fine and useful in bug reports.

If you cannot make the declaration, you can still help: spread the word, test
releases, or discuss UX ideas *in terms of Griffel's own behavior* — just let
the maintainers know about the conflict.

## Why this rule exists

Griffel's specification (`docs/10-provenance.md`) documents how it was built:
the behavior was studied through public documentation and the running
application, then implemented from the specification alone, without ever
reading Apostrophe's code. This protects the project (and every contributor)
from copyright-contamination claims. The declaration keeps every contribution
covered by the same guarantee.

## Legal

- Griffel is MIT-licensed. By contributing you agree that your contributions
  are licensed under the MIT License.
- Dependencies must be permissively licensed (MIT, Apache-2.0, BSD, ISC, …).
  LGPL system libraries are acceptable only as dynamically linked system
  packages. GPL/AGPL code is not accepted into the tree.
- External tools (Pandoc, Typst, XeLaTeX) are invoked as separate processes
  and must never be bundled or linked.

## Practical notes

- Keep every commit green: `cargo build --locked` and `cargo test --locked`
  must pass.
- Behavior changes should be reflected in (or proposed against) the relevant
  document in `docs/03-behavior/`; if the spec is ambiguous, note it rather
  than silently choosing an interpretation.
- UI text and comments are English-only.
