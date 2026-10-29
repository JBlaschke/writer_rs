// Serve fixture: template edits rebuild pages while the web view is skipped.
#import "@writer/doc:0.1.0" as base

#let writer-doc(..args, body) = {
    show heading: set text(fill: rgb("#1a3a8a"))
    base.writer-doc(..args, body)
}
