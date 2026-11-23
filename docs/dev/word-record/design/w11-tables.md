---
lang: en-GB
---

<!-- Tables (T114). HB to HD: today's XML (style Table, 1 pt rules at the
top and bottom, 0.5 pt under the header row, Compact cells with 1.8 pt
before and after, no top or bottom cell margin). HF and HH: two tables in a
row, with today's separator (an empty Normal paragraph). HM and HQ: the
same with R13 §3.4 M10's separator as raw OpenXML, before 9.5 pt and an
exact 0.7 pt line (10.2 pt in all, P). HS to HU: the first table again as
raw OpenXML with 3 pt rules at the top and bottom and 1.5 pt under the
header. HW and HX: today's table again, then HY1, a raw paragraph with a
direct 10.2 pt before (M10's space after a table). -->

HA1 EFH

| HB1 EFH | HB2 EFH |
|---------|---------|
| HC1 EFH | HC2 EFH |
| HD1 EFH | HD2 EFH |

HE1 EFH

| HF1 EFH | HF2 EFH |
|---------|---------|
| HG1 EFH | HG2 EFH |

| HH1 EFH | HH2 EFH |
|---------|---------|
| HK1 EFH | HK2 EFH |

HL1 EFH

| HM1 EFH | HM2 EFH |
|---------|---------|
| HN1 EFH | HN2 EFH |

```{=openxml}
<w:p><w:pPr><w:spacing w:before="190" w:after="0" w:line="14" w:lineRule="exact"/></w:pPr></w:p>
```

| HP1 EFH | HP2 EFH |
|---------|---------|
| HQ1 EFH | HQ2 EFH |

HR1 EFH

```{=openxml}
<w:tbl><w:tblPr><w:tblStyle w:val="Table"/><w:tblW w:w="0" w:type="auto"/><w:tblBorders><w:top w:val="single" w:sz="24" w:color="1F2328"/><w:bottom w:val="single" w:sz="24" w:color="1F2328"/></w:tblBorders><w:tblLook w:firstRow="1" w:lastRow="0" w:firstColumn="0" w:lastColumn="0" w:noHBand="0" w:noVBand="0" w:val="0020"/></w:tblPr><w:tblGrid><w:gridCol w:w="4819"/><w:gridCol w:w="4819"/></w:tblGrid>
<w:tr><w:trPr><w:tblHeader/></w:trPr><w:tc><w:tcPr><w:tcW w:w="4819" w:type="dxa"/><w:tcBorders><w:bottom w:val="single" w:sz="12" w:color="1F2328"/></w:tcBorders></w:tcPr><w:p><w:pPr><w:pStyle w:val="Compact"/></w:pPr><w:r><w:t xml:space="preserve">HS1 EFH</w:t></w:r></w:p></w:tc><w:tc><w:tcPr><w:tcW w:w="4819" w:type="dxa"/><w:tcBorders><w:bottom w:val="single" w:sz="12" w:color="1F2328"/></w:tcBorders></w:tcPr><w:p><w:pPr><w:pStyle w:val="Compact"/></w:pPr><w:r><w:t xml:space="preserve">HS2 EFH</w:t></w:r></w:p></w:tc></w:tr>
<w:tr><w:tc><w:tcPr><w:tcW w:w="4819" w:type="dxa"/></w:tcPr><w:p><w:pPr><w:pStyle w:val="Compact"/></w:pPr><w:r><w:t xml:space="preserve">HT1 EFH</w:t></w:r></w:p></w:tc><w:tc><w:tcPr><w:tcW w:w="4819" w:type="dxa"/></w:tcPr><w:p><w:pPr><w:pStyle w:val="Compact"/></w:pPr><w:r><w:t xml:space="preserve">HT2 EFH</w:t></w:r></w:p></w:tc></w:tr>
<w:tr><w:tc><w:tcPr><w:tcW w:w="4819" w:type="dxa"/></w:tcPr><w:p><w:pPr><w:pStyle w:val="Compact"/></w:pPr><w:r><w:t xml:space="preserve">HU1 EFH</w:t></w:r></w:p></w:tc><w:tc><w:tcPr><w:tcW w:w="4819" w:type="dxa"/></w:tcPr><w:p><w:pPr><w:pStyle w:val="Compact"/></w:pPr><w:r><w:t xml:space="preserve">HU2 EFH</w:t></w:r></w:p></w:tc></w:tr>
</w:tbl>
```

HV1 EFH

| HW1 EFH | HW2 EFH |
|---------|---------|
| HX1 EFH | HX2 EFH |

```{=openxml}
<w:p><w:pPr><w:pStyle w:val="BodyText"/><w:spacing w:before="204"/></w:pPr><w:r><w:t xml:space="preserve">HY1 EFH</w:t></w:r></w:p>
```
