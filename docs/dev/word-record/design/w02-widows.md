---
lang: en-GB
---

<!-- Widow control (T118). Every line is an exact 20 pt line with no
space around it, so a page holds 36 (728.5 pt of text area). Each test
starts a page: a filler paragraph of 35 or 32 lines, then a five-line
paragraph whose first line is the page's 36th or 33rd line. The page
the test lines land on shows whether Word keeps widows and orphans. -->

<!-- Test 1: default (no w:widowControl), orphan case. -->

:::x20 {custom-style="X20"}
HA01 EFH\
HA02 EFH\
HA03 EFH\
HA04 EFH\
HA05 EFH\
HA06 EFH\
HA07 EFH\
HA08 EFH\
HA09 EFH\
HA10 EFH\
HA11 EFH\
HA12 EFH\
HA13 EFH\
HA14 EFH\
HA15 EFH\
HA16 EFH\
HA17 EFH\
HA18 EFH\
HA19 EFH\
HA20 EFH\
HA21 EFH\
HA22 EFH\
HA23 EFH\
HA24 EFH\
HA25 EFH\
HA26 EFH\
HA27 EFH\
HA28 EFH\
HA29 EFH\
HA30 EFH\
HA31 EFH\
HA32 EFH\
HA33 EFH\
HA34 EFH\
HA35 EFH
:::

:::x20 {custom-style="X20"}
HB1 EFH\
HB2 EFH\
HB3 EFH\
HB4 EFH\
HB5 EFH
:::

<!-- Test 2: default (no w:widowControl), widow case. -->

:::pagebreak
:::

:::x20 {custom-style="X20"}
HC01 EFH\
HC02 EFH\
HC03 EFH\
HC04 EFH\
HC05 EFH\
HC06 EFH\
HC07 EFH\
HC08 EFH\
HC09 EFH\
HC10 EFH\
HC11 EFH\
HC12 EFH\
HC13 EFH\
HC14 EFH\
HC15 EFH\
HC16 EFH\
HC17 EFH\
HC18 EFH\
HC19 EFH\
HC20 EFH\
HC21 EFH\
HC22 EFH\
HC23 EFH\
HC24 EFH\
HC25 EFH\
HC26 EFH\
HC27 EFH\
HC28 EFH\
HC29 EFH\
HC30 EFH\
HC31 EFH\
HC32 EFH
:::

:::x20 {custom-style="X20"}
HD1 EFH\
HD2 EFH\
HD3 EFH\
HD4 EFH\
HD5 EFH
:::

<!-- Test 3: explicit on, orphan case. -->

:::pagebreak
:::

:::x20 {custom-style="X20"}
HE01 EFH\
HE02 EFH\
HE03 EFH\
HE04 EFH\
HE05 EFH\
HE06 EFH\
HE07 EFH\
HE08 EFH\
HE09 EFH\
HE10 EFH\
HE11 EFH\
HE12 EFH\
HE13 EFH\
HE14 EFH\
HE15 EFH\
HE16 EFH\
HE17 EFH\
HE18 EFH\
HE19 EFH\
HE20 EFH\
HE21 EFH\
HE22 EFH\
HE23 EFH\
HE24 EFH\
HE25 EFH\
HE26 EFH\
HE27 EFH\
HE28 EFH\
HE29 EFH\
HE30 EFH\
HE31 EFH\
HE32 EFH\
HE33 EFH\
HE34 EFH\
HE35 EFH
:::

```{=openxml}
<w:p><w:pPr><w:pStyle w:val="X20"/><w:widowControl/></w:pPr><w:r><w:t xml:space="preserve">HF1 EFH</w:t></w:r><w:r><w:br/></w:r><w:r><w:t xml:space="preserve">HF2 EFH</w:t></w:r><w:r><w:br/></w:r><w:r><w:t xml:space="preserve">HF3 EFH</w:t></w:r><w:r><w:br/></w:r><w:r><w:t xml:space="preserve">HF4 EFH</w:t></w:r><w:r><w:br/></w:r><w:r><w:t xml:space="preserve">HF5 EFH</w:t></w:r></w:p>
```

<!-- Test 4: explicit on, widow case. -->

:::pagebreak
:::

:::x20 {custom-style="X20"}
HG01 EFH\
HG02 EFH\
HG03 EFH\
HG04 EFH\
HG05 EFH\
HG06 EFH\
HG07 EFH\
HG08 EFH\
HG09 EFH\
HG10 EFH\
HG11 EFH\
HG12 EFH\
HG13 EFH\
HG14 EFH\
HG15 EFH\
HG16 EFH\
HG17 EFH\
HG18 EFH\
HG19 EFH\
HG20 EFH\
HG21 EFH\
HG22 EFH\
HG23 EFH\
HG24 EFH\
HG25 EFH\
HG26 EFH\
HG27 EFH\
HG28 EFH\
HG29 EFH\
HG30 EFH\
HG31 EFH\
HG32 EFH
:::

```{=openxml}
<w:p><w:pPr><w:pStyle w:val="X20"/><w:widowControl/></w:pPr><w:r><w:t xml:space="preserve">HK1 EFH</w:t></w:r><w:r><w:br/></w:r><w:r><w:t xml:space="preserve">HK2 EFH</w:t></w:r><w:r><w:br/></w:r><w:r><w:t xml:space="preserve">HK3 EFH</w:t></w:r><w:r><w:br/></w:r><w:r><w:t xml:space="preserve">HK4 EFH</w:t></w:r><w:r><w:br/></w:r><w:r><w:t xml:space="preserve">HK5 EFH</w:t></w:r></w:p>
```

<!-- Test 5: explicit off, orphan case. -->

:::pagebreak
:::

:::x20 {custom-style="X20"}
HL01 EFH\
HL02 EFH\
HL03 EFH\
HL04 EFH\
HL05 EFH\
HL06 EFH\
HL07 EFH\
HL08 EFH\
HL09 EFH\
HL10 EFH\
HL11 EFH\
HL12 EFH\
HL13 EFH\
HL14 EFH\
HL15 EFH\
HL16 EFH\
HL17 EFH\
HL18 EFH\
HL19 EFH\
HL20 EFH\
HL21 EFH\
HL22 EFH\
HL23 EFH\
HL24 EFH\
HL25 EFH\
HL26 EFH\
HL27 EFH\
HL28 EFH\
HL29 EFH\
HL30 EFH\
HL31 EFH\
HL32 EFH\
HL33 EFH\
HL34 EFH\
HL35 EFH
:::

```{=openxml}
<w:p><w:pPr><w:pStyle w:val="X20"/><w:widowControl w:val="0"/></w:pPr><w:r><w:t xml:space="preserve">HM1 EFH</w:t></w:r><w:r><w:br/></w:r><w:r><w:t xml:space="preserve">HM2 EFH</w:t></w:r><w:r><w:br/></w:r><w:r><w:t xml:space="preserve">HM3 EFH</w:t></w:r><w:r><w:br/></w:r><w:r><w:t xml:space="preserve">HM4 EFH</w:t></w:r><w:r><w:br/></w:r><w:r><w:t xml:space="preserve">HM5 EFH</w:t></w:r></w:p>
```

<!-- Test 6: explicit off, widow case. -->

:::pagebreak
:::

:::x20 {custom-style="X20"}
HN01 EFH\
HN02 EFH\
HN03 EFH\
HN04 EFH\
HN05 EFH\
HN06 EFH\
HN07 EFH\
HN08 EFH\
HN09 EFH\
HN10 EFH\
HN11 EFH\
HN12 EFH\
HN13 EFH\
HN14 EFH\
HN15 EFH\
HN16 EFH\
HN17 EFH\
HN18 EFH\
HN19 EFH\
HN20 EFH\
HN21 EFH\
HN22 EFH\
HN23 EFH\
HN24 EFH\
HN25 EFH\
HN26 EFH\
HN27 EFH\
HN28 EFH\
HN29 EFH\
HN30 EFH\
HN31 EFH\
HN32 EFH
:::

```{=openxml}
<w:p><w:pPr><w:pStyle w:val="X20"/><w:widowControl w:val="0"/></w:pPr><w:r><w:t xml:space="preserve">HP1 EFH</w:t></w:r><w:r><w:br/></w:r><w:r><w:t xml:space="preserve">HP2 EFH</w:t></w:r><w:r><w:br/></w:r><w:r><w:t xml:space="preserve">HP3 EFH</w:t></w:r><w:r><w:br/></w:r><w:r><w:t xml:space="preserve">HP4 EFH</w:t></w:r><w:r><w:br/></w:r><w:r><w:t xml:space="preserve">HP5 EFH</w:t></w:r></w:p>
```
