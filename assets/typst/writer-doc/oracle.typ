// Normal builds never evaluate the oracle calls in numbering.typ.
#let oracle-on = sys.inputs.at("writer-oracle", default: "0") == "1"

#let oracle(kind, it, number) = {
    if it.has("label") and not str(it.label).starts-with("writer-fn-") {
        [#metadata((kind: kind, label: str(it.label), number: number))#label(
            "writer-oracle-" + str(it.label)
        )]
    }
}

#let figure-kind(it) = {
    if it.kind == table { "table" }
    else if it.kind == raw { "listing" }
    else { "figure" }
}
