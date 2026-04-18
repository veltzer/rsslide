---
title: Alignment Examples
theme: default
paginate: true
---

<!--
align, title_align, content_align are all independent:
  align         — slide-level default (applies to title + content + bullets)
  title_align   — overrides alignment for the title only
  content_align — overrides alignment for content text and bullets only

Resolution order: title_align > align > "left"
                  content_align > align > "left"
-->

<!-- Slide-level align (all 9 combinations) -->

## Slide-level: left + top (default)

- Both title and bullets follow align
- Default — no properties needed

---

<!-- align: center -->
## Slide-level: center + top

- Title and bullets both centered
- Horizontal rule is suppressed for centered titles

---

<!-- align: right -->
## Slide-level: right + top

- Title and bullets both right-aligned

---

<!-- valign: middle -->
## Slide-level: left + middle

- Content block centred vertically
- Horizontal alignment is left

---

<!-- align: center, valign: middle -->
## Slide-level: center + middle

- Everything centred horizontally and vertically
- Great for section-break cards

---

<!-- align: right, valign: middle -->
## Slide-level: right + middle

- Right-aligned, vertically centred

---

<!-- valign: bottom -->
## Slide-level: left + bottom

- Content pushed to the bottom of the slide

---

<!-- align: center, valign: bottom -->
## Slide-level: center + bottom

- Centred and at the bottom

---

<!-- align: right, valign: bottom -->
## Slide-level: right + bottom

- Right-aligned at the bottom

---

<!-- Per-element overrides -->

<!-- title_align: center -->
## title_align: center · content_align: left (default)

- Title is centred; bullets stay left
- Horizontal rule is suppressed under the centred title

---

<!-- content_align: center -->
## title_align: left · content_align: center

- Title is left-aligned (default)
- Bullets are centred independently

---

<!-- title_align: right -->
## title_align: right · content_align: left

- Title pushed to the right
- Bullets remain left-aligned

---

<!-- content_align: right -->
## title_align: left · content_align: right

- Title on the left
- Bullets on the right

---

<!-- title_align: center, content_align: right -->
## title_align: center · content_align: right

- Centred title
- Right-aligned bullets

---

<!-- title_align: right, content_align: center -->
## title_align: right · content_align: center

- Right-aligned title
- Centred bullets

---

<!-- align as default, overridden per element -->

<!-- align: center, title_align: left -->
## align: center · title_align: left

- Slide default is center
- title_align overrides title to left
- Bullets inherit slide align (center)

---

<!-- align: right, content_align: left -->
## align: right · content_align: left

- Slide default is right
- Bullets overridden to left
- Title inherits slide align (right)
