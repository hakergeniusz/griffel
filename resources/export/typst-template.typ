// Griffel Typst export template (original work, MIT — same license as
// the project). Pandoc fills the template variables; no Typst math is
// used so the two syntaxes never collide.
#set page(
$if(papersize)$
  paper: "$papersize$",
$else$
  paper: "a4",
$endif$
  margin: (x: 2.2cm, y: 2.4cm),
)
#set text(size: 11pt)
#set heading(numbering: $if(section-numbering)$"$section-numbering$"$else$none$endif$)
$if(toc)$
#outline()
#v(1.2em)
$endif$
$if(title)$
#align(center)[#text(17pt, weight: 700)[$title$]]
#v(0.6em)
$endif$
$body$
