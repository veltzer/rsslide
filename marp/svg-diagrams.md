---
title: SVG Diagrams Demo
theme: default
paginate: true
---

## Inline SVG — Simple Shapes

<svg viewBox="0 0 400 200" xmlns="http://www.w3.org/2000/svg">
  <rect x="10" y="10" width="120" height="60" rx="8" fill="#4A90D9"/>
  <text x="70" y="45" text-anchor="middle" fill="white" font-size="16" font-family="sans-serif">Input</text>
  <line x1="130" y1="40" x2="180" y2="40" stroke="#333" stroke-width="2" marker-end="url(#arr)"/>
  <rect x="180" y="10" width="120" height="60" rx="8" fill="#7ED321"/>
  <text x="240" y="45" text-anchor="middle" fill="white" font-size="16" font-family="sans-serif">Process</text>
  <line x1="300" y1="40" x2="350" y2="40" stroke="#333" stroke-width="2" marker-end="url(#arr)"/>
  <rect x="350" y="10" width="40" height="60" rx="8" fill="#D0021B"/>
  <text x="370" y="45" text-anchor="middle" fill="white" font-size="12" font-family="sans-serif">Out</text>
  <defs>
    <marker id="arr" markerWidth="10" markerHeight="7" refX="10" refY="3.5" orient="auto">
      <polygon points="0 0, 10 3.5, 0 7" fill="#333"/>
    </marker>
  </defs>
</svg>

---

## Inline SVG — Pie Chart

<svg viewBox="0 0 300 200" xmlns="http://www.w3.org/2000/svg">
  <circle cx="100" cy="100" r="80" fill="#4A90D9"/>
  <path d="M100,100 L100,20 A80,80 0 0,1 169,60 Z" fill="#7ED321"/>
  <path d="M100,100 L169,60 A80,80 0 0,1 155,172 Z" fill="#F5A623"/>
  <text x="200" y="60"  font-size="13" font-family="sans-serif">■ <tspan fill="#7ED321">25%</tspan></text>
  <text x="200" y="85"  font-size="13" font-family="sans-serif">■ <tspan fill="#F5A623">22%</tspan></text>
  <text x="200" y="110" font-size="13" font-family="sans-serif">■ <tspan fill="#4A90D9">53%</tspan></text>
</svg>

Inline SVG with no external dependencies.

---

## SVG from File

![rust](assets/icons/rust.svg)

The Rust logo loaded from assets/icons/rust.svg

---

## SVG + Bullets

<svg viewBox="0 0 400 80" xmlns="http://www.w3.org/2000/svg">
  <rect width="400" height="80" fill="#f5f5f5" rx="6"/>
  <rect x="10"  y="20" width="80" height="40" rx="4" fill="#4A90D9"/>
  <text x="50"  y="44" text-anchor="middle" fill="white" font-family="sans-serif" font-size="13">Parser</text>
  <rect x="130" y="20" width="80" height="40" rx="4" fill="#7ED321"/>
  <text x="170" y="44" text-anchor="middle" fill="white" font-family="sans-serif" font-size="13">Model</text>
  <rect x="250" y="20" width="80" height="40" rx="4" fill="#D0021B"/>
  <text x="290" y="44" text-anchor="middle" fill="white" font-family="sans-serif" font-size="13">Exporter</text>
  <line x1="90"  y1="40" x2="130" y2="40" stroke="#333" stroke-width="2"/>
  <line x1="210" y1="40" x2="250" y2="40" stroke="#333" stroke-width="2"/>
</svg>

- SVG diagram above, bullets below
- Both rendered on the same slide
