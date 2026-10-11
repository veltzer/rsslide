# Code highlighting

Code blocks are highlighted to match marp, which colours them with
highlight.js 11.11.1 (the version marp-cli bundles). rsslide colours with
syntect, and decides *whether* to colour a block the way marp does.

## Which blocks are coloured

The fence language is looked up, in lower case as highlight.js does, in
`assets/syntaxes/highlightjs-languages.txt`: highlight.js's language names
and the aliases each one owns.

- Not listed (`misc`, `output`, `hcl`, `nasm`, `conf`, ...): plain text,
  as in marp. These are not errors: marp shows them plain too.
- Listed: rsslide must have a syntax for it. One that is missing is a hard
  error naming the language, never a quietly plain block.
- `shell` (`console`, `shellsession`) is a shell session, not a syntax:
  as in highlight.js, a line that starts with a prompt
  (`^\s{0,3}[/~\w\d[\]()@-]*[>%$#][ ]?`) is the prompt and then a bash
  command, continued over lines ending in `\`; every other line is output
  and stays plain.

## Where the syntaxes come from

1. [two-face](https://codeberg.org/CosmicHarper/two-face): bat's syntax
   collection, a superset of syntect's defaults, in its fancy-regex build
   (rsslide's regex engine; Oniguruma would add a C dependency to every
   release target).
2. `assets/syntaxes/*.sublime-syntax`, for highlight.js languages two-face
   lacks under fancy-regex: Device Tree (`dts`), ARM assembly (`arm`) and
   MIPS assembly (`mips`). Each is transcribed from highlight.js's grammar,
   with scopes chosen so the bundled GitHub theme gives highlight.js's
   colours (title -> `entity.name.class`, attr/meta/number/operator ->
   blue `constant`/`keyword.operator`, built_in/symbol ->
   `support.function`/`constant.other.symbol`). They are read by
   `src/exporter/syntax_file.rs`, which accepts only the subset of the
   format they use (`match`, `scope`, `captures`, `push`, `pop`, `include`,
   `meta_scope`): syntect's own loader needs `yaml-rust`, which is
   unmaintained (RUSTSEC-2024-0320).

A language is found by an explicit name (`SYNTAX_NAMES` in
`src/exporter/highlight.rs`, for `apache`, `armasm`, `dts`, `mipsasm`),
else by syntect token lookup of the fence token, the highlight.js name, and
then its aliases (so `csharp` finds C# through `cs`, `jsonc` finds JSON).

The theme is `assets/themes/github.tmTheme` (`code.theme` in
`rsslide.toml`), with marp's GitHub colours. Token colours can still differ
from marp where a bat grammar scopes a token differently from highlight.js
(nginx and Apache directives, bash builtins such as `ls`).

## Regenerating the language table

After a marp-cli upgrade, from marp-cli's install directory:

```bash
node -e '
const h=require(require.resolve("highlight.js",{paths:[require.resolve("@marp-team/marp-core")]}));
for(const n of h.listLanguages().sort()){const L=h.getLanguage(n);
console.log([n,...new Set((L.aliases||[]).map(a=>a.toLowerCase()))].filter((a,i)=>i==0||(a!==n&&h.getLanguage(a)===L)).join(" "));}'
```

Keep the header comment of the file, and update the version in it. An alias
two languages claim belongs to the one `getLanguage` returns (the later
registration), which the script already applies.
