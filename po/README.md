# Translating Griffel

Griffel uses GNU gettext. English is the source language and always
present; every other language ships as a `po/<lang>.po` catalog that is
compiled into the binary at build time. Untranslated strings fall back
to English automatically, so a partial translation is always safe.

## Adding a language

1. Copy the template: `cp po/griffel.pot po/de.po` (use your [language
   code](https://www.gnu.org/software/gettext/manual/html_node/Language-Codes.html);
   add a country variant when it matters, e.g. `pt_BR`).
2. Fill in the header: `Language: de`, the plural rule, your name in
   `Last-Translator`. The plural rule must be a correct
   [Plural-Forms](https://www.gnu.org/software/gettext/manual/html_node/Plural-forms.html)
   expression — plural strings will not localize without it.
3. Translate the `msgstr` fields. Rules:
   - Keep `{}` placeholders exactly as they are, in the same order
     (`"{} of {} {}"` → `"{} von {} {}"`). They are replaced at runtime
     in order.
   - Strings with markup (the release notes) keep their HTML tags.
   - Leave format/product names untranslated when there is no
     established translation (`LaTeX`, `EPUB v3`, `CommonMark`, …).
4. Build: `cargo build`. That's all — the build script finds every
   `po/*.po`, compiles it with `msgfmt` and embeds it. At runtime the
   user's `LANGUAGE`/`LC_ALL`/`LC_MESSAGES`/`LANG` environment picks the
   catalog (`de_DE.UTF-8` matches `de`).
5. Check your work with the real UI: `LANGUAGE=de cargo run`.

To test a translation without switching your whole system, prefix the
command with `LANGUAGE=<code>`.

## Regenerating the template

After strings change in the sources:

```sh
scripts/update-pot.sh
```

Then merge the changes into existing catalogs with
`msgmerge --update po/de.po po/griffel.pot` (obsolete entries can be
deleted; fuzzy entries need review before they are used).

## Notes

- The build needs `msgfmt` (the `gettext` tools package). Without it,
  Griffel builds English-only with a warning.
- The tutorial document (`resources/tutorial.md`) and the GSettings
  schema descriptions are not translated yet.
- Translations are licensed like the rest of Griffel (MIT).
