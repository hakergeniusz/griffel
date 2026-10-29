#!/bin/sh
# Regenerates the bundled reference documents for ODT/DOCX export page
# sizes (`03-behavior/08-export.md` §1/§2.1).
#
# The documents are produced by running pandoc itself (its documented
# "modify a reference document produced by pandoc" workflow) and then
# patching the page size in the generated XML. No pandoc data files are
# redistributed.
set -e
cd "$(dirname "$0")"
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

printf 'Title\n=====\n\nBody text.\n' > "$work/in.md"

patch() { # $1 = archive  $2 = width  $3 = height
python3 - "$1" "$2" "$3" <<'PY'
import re, sys, zipfile
path, w, h = sys.argv[1], sys.argv[2], sys.argv[3]
src = zipfile.ZipFile(path)
items = {n: src.read(n) for n in src.namelist()}
src.close()
out = zipfile.ZipFile(path, "w", zipfile.ZIP_DEFLATED)
for n, data in items.items():
    if n == "styles.xml":  # ODT: fo:page-width/height in the page layout
        s = data.decode()
        s = re.sub(r'fo:page-width="[^"]*"', f'fo:page-width="{w}in"', s)
        s = re.sub(r'fo:page-height="[^"]*"', f'fo:page-height="{h}in"', s)
        data = s.encode()
    elif n == "word/document.xml":  # DOCX: insert pgSz into sectPr
        s = data.decode()
        pgsz = f'<w:pgSz w:w="{w}" w:h="{h}"/>'
        if "<w:pgSz" in s:
            s = re.sub(r'<w:pgSz[^/]*/>', pgsz, s)
        else:
            s = s.replace("</w:sectPr>", f"{pgsz}</w:sectPr>", 1)
        data = s.encode()
    out.writestr(n, data)
out.close()
PY
}

for fmt in odt docx; do
  pandoc "$work/in.md" -o "$work/base.$fmt"
done

# ODT sizes in inches; DOCX sizes in twips (1440 per inch).
cp "$work/base.odt"  reference-odt-a4.odt;     patch reference-odt-a4.odt     8.27 11.69
cp "$work/base.odt"  reference-odt-letter.odt; patch reference-odt-letter.odt 8.5  11
cp "$work/base.docx" reference-docx-a4.docx;   patch reference-docx-a4.docx   11906 16838
cp "$work/base.docx" reference-docx-letter.docx; patch reference-docx-letter.docx 12240 15840

echo "reference documents regenerated:"
ls -la reference-*.odt reference-*.docx
