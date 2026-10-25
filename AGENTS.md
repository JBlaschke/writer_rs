# Code style

Use these preferences for code you write or edit, across languages. They are
intentional and may differ from a language's conventional style. Aim for
compact, readable code with clear visual structure. Apply judgment, not
mechanical rules.

**The examples use Julia and Python, but these style rules apply to all
languages.** Adapt the layout to each language's syntax; do not limit these
preferences to Julia or Python files.

## Indentation and line length

- Use **4 spaces** per indentation level, not tabs, except where the language
  or required tooling mandates otherwise.
- Aim for **about 80 characters per line**, including indentation. This is a
  soft target. A modest overrun is preferable to awkward wrapping, cryptic
  names, or unnecessary intermediate variables.
- Keep short calls and function signatures on one line when they read
  naturally.

## One action per line; align related code

- Put independent statements or actions on separate lines. Do not combine
  mutations just because they update related state. Avoid semicolon-separated
  statements and other tricks that squeeze sequential actions onto one line.
- This does not prohibit multiple arguments on a line, ordinary nested
  expressions, or semicolons required by syntax, such as Julia keyword argument
  separators or C-style loop headers.
- Vertically align related assignments, field declarations, type annotations,
  and named arguments when a small local group benefits from it.
- Keep alignment local and restrained. Do not add excessive padding, align
  unrelated code, or split compact argument groups solely to align their `=`,
  except for the declaration layout with default values described below.

For example:

```julia
used         += 1
kept         += 1
buffer[used]  = value

push!(result.x, x)
push!(result.y, y)
```

## Calls, constructors, and argument grouping

- When a call needs wrapping, keep the function name and opening `(` together,
  then start the arguments on the next line.
- Indent argument lines **4 spaces beyond the containing statement's base
  indentation**. Never align them under the opening parenthesis or first
  argument.
- Put the closing `)` on its own line, aligned with the containing statement's
  base indentation. Keep any required punctuation with it.
- Prefer **several short, related arguments on each line**. This applies to
  positional arguments, keyword/named arguments, and declaration parameters,
  with an exception for declarations that specify default values (see below).
- Break near the width target, or earlier when it keeps a meaningful group
  together. Do not fill every line to capacity at the expense of grouping.
- Use one argument per line when arguments are long or complex, or when a
  vertical list materially improves readability. It is not the default simply
  because a call or signature spans multiple lines.
- Apply the same rules to constructor calls and object initialization. Group
  short related fields where syntax allows; give complex fields their own
  lines.

```julia
next_edges, splits, merges = refine_edges(
    edges, counts;
    max_cells, min_cells, min_width = width,
    split_factor, merge_factor, merge_tolerance
)

grid = learn_grid(
    source;
    initial_cells = 32, max_cells = 256, min_cells = 6,
    split_factor = 1.5, merge_factor = 0.5, merge_tolerance = 0.25
)
```

## Wrapped function declarations and definitions

Declarations use extra indentation to distinguish parameters from the body.
Relative to the indentation of the declaration itself:

- Parameter lines: **12 spaces** (three indentation levels).
- Closing `)`: **8 spaces** (two indentation levels).
- Function body: **4 spaces** (one indentation level).

Keep the opening `(` on the declaration line. Keep necessary syntax such as a
colon, return annotation, or opening brace with the closing `)` where
appropriate. This applies to functions, methods, and constructor definitions.
Calls still use the ordinary call layout above. All offsets are relative,
including in nested code.

```julia
function refine_edges(
            edges, counts; max_cells, min_cells, min_width,
            split_factor, merge_factor, merge_tolerance
        )
    target = sum(counts) / max_cells
    widths = diff(edges)
    # ...
end
```

For declarations that specify default values, **one parameter per line is also
fine**, even when the parameters are short. Align related defaults when
helpful. This is an option, not a requirement: compact groups remain fine, and
required parameters can stay grouped while parameters with defaults get their
own lines.

For example, in Python:

```python
def build_report(
            records, columns, filters,
            title          = "Summary",
            include_totals = True,
            precision      = 2
        ):
    rows = select_rows(records, columns, filters)
    # ...
```

## Rust refinements

- Wrapped Rust function and method declarations follow the repository's stable
  rustfmt configuration. Use its normal parameter indentation and closing
  delimiter layout instead of the 12/8-space declaration layout above. Do not
  add `#[rustfmt::skip]` solely to enforce that declaration layout.
- In Rust, URLs, hash literals, and exact-output strings are explicit exceptions
  to the 80-character target. Keep them intact when splitting or rewriting them
  would make verification harder or risk changing their bytes.

## Apply with judgment

Preserve language syntax and program behavior. These examples illustrate
layout, not naming conventions, API design, or a requirement to rewrite code
into Julia idioms. Use the equivalent layout where a language has different
delimiters.

Apply the style to new and changed code without reformatting unrelated
sections. Do not run a formatter that silently replaces these choices with its
defaults. Respect mandatory tooling constraints; where formatting is
configurable, prefer these rules. Readability and useful grouping take
precedence over rigid column limits, excessive alignment, or blanket
one-argument-per-line formatting.
