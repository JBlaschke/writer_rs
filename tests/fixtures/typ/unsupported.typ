<orphan>
#import "@writer/doc:9.9.9": *
#import "unloaded.typ": outsider
#show: writer-doc.with(title: upper("dynamic"), bibliography: "refs.bib")
#show: writer-doc
#set circle(radius: 2pt)
#let closure(x) = [#x]
#let args = ([A], [B])

A supported paragraph survives.

#outsider()
#closure(1)
#unknown
#(1 + 2)
#context [Unevaluated context.]
#list(..args)
#raw(42)
#heading(level: 8, bogus: true)[Clamped heading]

#figure([Body], kind: "custom")

#table(columns: 1, [First paragraph.

Second paragraph.])

Text cannot hold this label <ignored>.

#block[#set text(lang: "de")
Nested setting.]

#include ("missing" + ".typ")
