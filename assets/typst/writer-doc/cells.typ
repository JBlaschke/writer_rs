// Execution counts and prompts belong to HTML/DOCX, not the printed document.
#let code-cell(index: 0, count: none, lang: "", input: none, ..outputs) = {
    if input != none { input }
    for output in outputs.pos() {
        parbreak()
        output
    }
}

// The provider supplies saved content; this helper never executes notebook code.
#let include-notebook(
            file, cells: auto, tags: none,
            hide-input: auto, hide-output: auto
        ) = {
    if type(file) != path {
        panic("include-notebook: pass path(\"x.ipynb\") instead of " + repr(file))
    }
    let refs = if cells == auto {
        none
    } else if type(cells) == array {
        cells
    } else {
        (cells,)
    }
    if refs != none {
        for reference in refs {
            if not ((type(reference) == int and reference >= 0)
                or type(reference) == str) {
                panic("include-notebook: invalid cell reference " + repr(reference))
            }
        }
    }
    let selected-tags = if tags == none {
        none
    } else if type(tags) == str {
        (tags,)
    } else if type(tags) == array {
        tags
    } else {
        panic("include-notebook: invalid tags value " + repr(tags))
    }
    if selected-tags != none {
        for tag in selected-tags {
            if type(tag) != str {
                panic("include-notebook: invalid tag " + repr(tag))
            }
        }
    }
    for (name, value) in (
        ("hide-input", hide-input), ("hide-output", hide-output)
    ) {
        if value != auto and type(value) != bool {
            panic("include-notebook: invalid " + name + " value " + repr(value))
        }
    }

    // Content fields are evaluated eagerly by the generated module.
    import file: writer-cells
    let selected = ()
    if refs != none {
        for reference in refs {
            let found = writer-cells.find(c => if type(reference) == int {
                c.index == reference
            } else {
                c.id == reference
            })
            if found == none {
                panic("include-notebook: cell " + repr(reference) + " was not found or is removed")
            }
            selected.push(found.index)
        }
    }
    for c in writer-cells {
        if (refs == none or c.index in selected) and (
            selected-tags == none or c.tags.any(tag => tag in selected-tags)
        ) {
            let body = if c.kind != "code" {
                c.body
            } else if hide-input == true and hide-output == true {
                none
            } else if hide-input == true {
                c.outputs
            } else if hide-output == true {
                c.input
            } else {
                c.body
            }
            if body != none { body }
        }
    }
}
