#!/bin/sh
# Regenerate po/griffel.pot from the strings marked in the sources
# (i18n(), i18n_f(), i18n_n(), i18n_nf() — see src/i18n.rs and
# po/README.md). Run from the repository root:  scripts/update-pot.sh
set -e

cd "$(dirname "$0")/.."

xgettext \
    --language=Rust \
    --from-code=UTF-8 \
    --keyword=i18n \
    --keyword=i18n_f:1 \
    --keyword=i18n_n:1,2 \
    --keyword=i18n_nf:1,2 \
    --directory=. \
    --files-from=po/POTFILES.in \
    --package-name=Griffel \
    --package-version="$(grep -m1 '^version' Cargo.toml | sed 's/.*"\(.*\)"/\1/')" \
    --msgid-bugs-address=https://github.com/hakergeniusz/griffel/issues \
    --output=po/griffel.pot

# A fresh pot drops the previous file; keep the header tidy.
sed -i 's/^# SOME DESCRIPTIVE TITLE\./# Griffel translations template./' po/griffel.pot
echo "po/griffel.pot updated: $(grep -c '^msgid' po/griffel.pot) msgids"
