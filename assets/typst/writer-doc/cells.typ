// Execution counts and prompts belong to HTML/DOCX, not the printed document.
#let code-cell(index: 0, count: none, lang: "", input: none, ..outputs) = {
    if input != none { input }
    for output in outputs.pos() {
        parbreak()
        output
    }
}

// The include provider and notebook selection implementation arrive in T53.
#let include-notebook(
            file, cells: auto, tags: none,
            hide-input: auto, hide-output: auto
        ) = {
    panic("include-notebook: notebook includes are not implemented yet (T53)")
}
