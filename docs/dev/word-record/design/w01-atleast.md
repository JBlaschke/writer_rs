---
lang: en-GB
---

<!-- Where an atLeast line puts its extra height (T106). The HB and HD
paragraphs are raw OpenXML: Source Sans 3 11 pt (single line 15.664 pt) at
w:line="600" (30 pt, more than a single line) and w:line="200" (10 pt,
less). The X12 fences are exact 12.1 pt lines. -->

:::x12 {custom-style="X12"}
HA1 EFH\
HA2 EFH
:::

```{=openxml}
<w:p><w:pPr><w:spacing w:before="0" w:after="0" w:line="600" w:lineRule="atLeast"/></w:pPr><w:r><w:t xml:space="preserve">HB1 EFH</w:t></w:r><w:r><w:br/></w:r><w:r><w:t xml:space="preserve">HB2 EFH</w:t></w:r></w:p>
```

:::x12 {custom-style="X12"}
HC1 EFH\
HC2 EFH
:::

```{=openxml}
<w:p><w:pPr><w:spacing w:before="0" w:after="0" w:line="200" w:lineRule="atLeast"/></w:pPr><w:r><w:t xml:space="preserve">HD1 EFH</w:t></w:r><w:r><w:br/></w:r><w:r><w:t xml:space="preserve">HD2 EFH</w:t></w:r></w:p>
```

:::x12 {custom-style="X12"}
HE1 EFH\
HE2 EFH
:::
