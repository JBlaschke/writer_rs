---
lang: en-GB
---

<!-- Lists (T112). HB to HF: today's XML (tight items are Compact, 1.8 pt
before and after; loose items and continuation paragraphs are Body Text).
HN and HP: R13 §3.4 M4 as raw OpenXML, the LI style (before 0, after
2.75 pt) with a direct after of P (204 twips) on the last top-level item;
they reuse the bullet numbering of the HB and HC lists. -->

HA1 EFH

- HB1 EFH
- HB2 EFH
  - HC1 EFH
  - HC2 EFH
- HB3 EFH

HD1 EFH

- HE1 EFH

- HE2 EFH

- HE3 EFH

HF1 EFH

- HG1 EFH

  HG2 EFH

- HG3 EFH

HH1 EFH

HK1 EFH
:   HL1 EFH

HK2 EFH
:   HL2 EFH

HM1 EFH

```{=openxml}
<w:p><w:pPr><w:pStyle w:val="LI"/><w:numPr><w:ilvl w:val="0"/><w:numId w:val="1001"/></w:numPr></w:pPr><w:r><w:t xml:space="preserve">HN1 EFH</w:t></w:r></w:p>
<w:p><w:pPr><w:pStyle w:val="LI"/><w:numPr><w:ilvl w:val="0"/><w:numId w:val="1001"/></w:numPr></w:pPr><w:r><w:t xml:space="preserve">HN2 EFH</w:t></w:r></w:p>
<w:p><w:pPr><w:pStyle w:val="LI"/><w:numPr><w:ilvl w:val="1"/><w:numId w:val="1002"/></w:numPr></w:pPr><w:r><w:t xml:space="preserve">HP1 EFH</w:t></w:r></w:p>
<w:p><w:pPr><w:pStyle w:val="LI"/><w:numPr><w:ilvl w:val="1"/><w:numId w:val="1002"/></w:numPr></w:pPr><w:r><w:t xml:space="preserve">HP2 EFH</w:t></w:r></w:p>
<w:p><w:pPr><w:pStyle w:val="LI"/><w:numPr><w:ilvl w:val="0"/><w:numId w:val="1001"/></w:numPr><w:spacing w:after="204"/></w:pPr><w:r><w:t xml:space="preserve">HN3 EFH</w:t></w:r></w:p>
```

HQ1 EFH
