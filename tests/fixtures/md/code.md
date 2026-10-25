---
title: Code attributes and listing figures
# code.md: markdown-dialect.md §4.6
---

```python {.numbered start=10 highlight="11"}
print("first")
print("highlighted")
```

```rust {.plain}
fn main() {}
```

```python {highlight="1-3,5"}
a = 1
b = 2
c = 3
d = 4
e = 5
```

```python {lines="1-3|5"}
a = 1
b = 2
c = 3
d = 4
e = 5
```

```python {#lst-x caption="A labelled listing" include=include/part-b.md .custom trim=true escape=false}
print("a listing")
```

```python {caption="A caption without an explicit label"}
print("captioned")
```

```python {#lst-uncaptioned}
print("labelled")
```

```python {start=0 highlight="3-1" lines="a"}
print("invalid attributes are retained but ignored")
```

```python {start=x highlight="1,,2"}
print("more invalid values")
```

~~~rust
let value = 1;
~~~

An indented code block follows.

    plain indented code

```
A fence without a language.
```
