---
title: Raw passthrough formats
# raw.md: markdown-dialect.md §4.8
---

```{=html}
<section>Raw HTML</section>
```

```{=latex}
\clearpage
```

```{=typst}
#pagebreak()
```

```{=openxml}
<w:p><w:r><w:t>Raw Word text</w:t></w:r></w:p>
```

```{=weird}
An unknown format is retained.
```

<div>
<img src="images/plot.png">
</div>

<!-- A comment is dropped. -->

Inline raw `<em>inline HTML</em>`{=html}.
