# `http_headers` — serde deserializer implementation plan (rev 2)

A tick-off timeline for adding a custom `serde::Deserializer` to `http_headers`, mapping
HTTP/1.1 header field values (RFC 9110) onto Serde's data model on top of the existing
zero-copy, stack-based parser in `line.rs`.

Work is ordered by strict dependency. **Value-level deserialization comes first; identifier
matching follows immediately after the core body as its own phase, before any struct/enum work
that depends on it.**

Rev 2 (feedback incorporated):
- Deserializer-specific errors live in a **new standalone error type** — `error.rs` / `ErrorValue` are not modified.
- **No refactor of `line.rs`.** Iteration and line-parsing logic stay encapsulated there; the deserializer consumes only the public `Headers`/`HeaderLine`/`HeaderValues`/`Value` API, using a two-call lookahead at the call site for shape detection.
- Docs, prelude exports, and comment polish are **out of scope** (owned separately); this implementation is a starting point, not a final version.

---

## Locked design decisions (reference)

### Shape → Serde data model
| Input shape | Data model |
|---|---|
| Header section | map: lower-cased name → value |
| Comma-separated list | seq of items |
| Bare element (no `,`, no `;`) | str / numeric, per hint |
| Empty field value (`Name:` / `Name: ;`) | `Option::None` |
| Parameters only (leading `;`, no element) | map of key → value |
| Element + parameters | struct/variant: element = **name**, parameters = **fields** |

### Hint semantics (mismatch matrix)
- `str`/`string`/`bytes`: a bare-element scope is required.
  - Value position, single bare element → borrowed slice (zero-copy).
  - Value position, list of bare elements only → **owned `", "`-rejoin** of item slices (the one documented allocation; whole-line borrowing is unavailable without parser access).
  - Item position (inside a seq) → the item's element slice (borrowed).
  - Any scope carrying parameters (element+params, params-only, or a list containing a parameterized item) → **hard error** — exact wire form is not reconstructable from the value stream (quoted param values are unquoted in place), so error rather than silently drop.
- numeric: single bare element only; strict ASCII digits, optional leading `-` for signed; error otherwise.
- `bool`, floats: **rejected in v1** (clear unsupported error).
- `seq`/`tuple`: bare single value → 1-element seq; list → items (grouped by element-as-boundary); params-only → **1-element seq of one map holding all params** (list boundaries between consecutive params-only items are indistinguishable in the value stream and flatten — documented limitation); element+params → per-item struct/enum.
- `map`/`struct`: single item only (list at value position → hard error). Bare element → validate name + zero-entry map; params-only → single map (a list of params-only items flattens into it); element+params under a plain `map` hint → **hard error**; an `Element` encountered while draining a params-only map (mixed list) → **hard error**.
- `enum` (externally tagged only): single item only. Bare element → unit variant; element+params → newtype/struct variant (identifier + param map); params-only → **error** (`MissingElement`). Internally/adjacently-tagged and untagged are unsupported.
- `option`: peek — empty field value → `None`, else `Some(self)`.
- `any`: bare→str, list→seq, params-only→map, element+params→**hard error**, empty→unit.
- `ignored_any`: advance without visiting.

### Identifier matching & normalization
- In `deserialize_identifier`, when the enclosing hint carried an expected-name list (enum `variants` or struct `fields`; applies at all three levels: section names, variant names, parameter keys): compare `normalize(expected)` vs `normalize(wire)`; **first match wins** on collisions; feed the **exact expected name** to the visitor. On no match, feed **raw wire bytes** (so `rename`/`alias` still work) → serde's standard unknown-variant/unknown-field error.
- `normalize` = strip all non-alphanumerics + lowercase. Bridges `text/plain`↔`TextPlain`, `max-age`↔`max_age`, `content-type`↔`content_type`.
- **No normalization is added to the parser.** Field names and parameter keys stay lower-cased in place as today; element bytes are never mutated (match-time comparison only — element bytes never surface as data on typed paths).

### Errors
- A **new standalone error type** for the deserializer layer (name/location to be settled with the prelude work; suggested home: the deserializer module). It is the deserializer's `type Error` and implements `Display`, `Debug`, `std::error::Error`, and `serde::de::Error` (the latter supplying `custom`/`invalid_*` for serde-side failures such as unknown variants).
  - A variant wrapping parser errors (`error::Error`), with `From<error::Error>` — parsing errors surfaced while driving `HeaderValues` still propagate out of deserialization.
  - An **`ErrorParsing`** variant grouping the deserialization-specific failures: `MissingElement`, element/name mismatch, invalid UTF-8, unsupported shape (parameterized value where a bare element is required; `any` × element+params), unsupported hint (bool/float in v1), invalid integer (numeric hint on a non-numeric element), and a `Custom` catch-all for serde-reported failures (`de::Error::custom`, e.g. unknown variant).
- `error.rs` / `ErrorValue` are **not modified** — low-level parser consumers never see serde-specific variants.

### Borrowing / lifetime model
- The deserializer wraps a `HeaderValues<'a>` iterator plus **at most one buffered (lookahead) `Value`** and an item-scope flag set by `SeqAccess`. No parser internals are exposed or refactored.
- Shape detection at the call site via the two-call pattern: first `next()` = head; second `next()` discriminates — `None` → single value, `Some(..)` → list; head being `Element` vs `Parameter` distinguishes element-bearing from params-only. Remaining values are drained in a loop over `next()`. Items group by element-as-boundary: an `Element` starts a new item; following `Parameter`s belong to it until the next `Element` or end.
- Values are moved out of the iterator and their `&'a mut [u8]` slices coerced to shared `&'a` views at emit time. Consumption is **forward-only**: once a region has been yielded it is frozen, so the serde path and the mutating iterator remain alternative consumers of one parser instance.

### Scope boundaries (out of scope for this layer)
- Directive-style headers (`Cache-Control: max-age=604800`, no leading `;`) parse as plain elements/strings here; typed directive parsing is userland. The leading-`;` requirement for params-only is a documented sharp edge.

---

## Standing constraints (apply to every item)
- Workspace lints are strict: **no `unwrap`/`expect`** (deny), **no shadowing** (forbid), **no `print*`/`dbg!`** (deny/forbid). Use `?` and explicit matching.
  - Finding (verified on the current nightly toolchain): `unwrap_used`/`expect_used` are enforced in non-test code but **exempted from `#[cfg(test)]` code** — existing crate tests use `.unwrap()` freely, so new tests follow that house style.
- Zero-copy except the one documented allocation (owned rejoin for str/bytes over bare-element lists).
- **`line.rs` encapsulation**: no refactors exposing parser internals; the deserializer uses only the public API of the existing types.
- The existing public API keeps **exactly** its current observable behavior — all existing tests in `line.rs`/`stack.rs` stay green.
- No doc/comment polish is required; the implementation is a starting point.

---

## Phase 0 — Foundations & scaffolding

- [x] Add `mod deserializer;` to `src/lib.rs` (visibility settled later alongside the prelude work). *(Verified pre-existing: declaration at lib.rs:5, placeholder file present.)*
- [x] New standalone error type per the Errors section: parser-error wrapper variant + `From<error::Error>`; `ErrorParsing` grouping the serde-specific failures; `Display`/`Debug`/`std::error::Error`/`serde::de::Error` impls. **No changes to `error.rs`.**
  - Findings: (1) two variants beyond the original list were required — `InvalidInteger` (numeric hints need a content failure distinct from shape failures) and `Custom(String)` (`de::Error::custom` must return `Self`; serde-generated code calls it for unknown variants / invalid values); (2) types are declared `pub(crate)` (matches the `stack.rs` precedent, avoids `unreachable_pub` warnings) pending the prelude visibility work; (3) two transient `dead_code` warnings on the enums remain in the non-test lib build until Phase 1a references them.
- [x] Confirm `serde` is a normal dependency (it is: `serde.workspace = true`); `derive` stays dev-only. No `Cargo.toml` change expected — verify. *(Verified: compiles with `default-features = false`; no Cargo.toml changes made.)*

## Phase 1 — Value-level deserializer

### 1a. Skeleton, lookahead discipline, borrowing mechanics
- [x] Deserializer struct owning: the field-value region (lazily initialized into a `HeaderValues<'a>`), one lookahead `Option<Value<'a>>`, and an item-scope flag. Entry point following serde's `from_*` convention.
  - Findings: (1) deviation from "lazily initialized" — every hint path (including `str`) classifies via the parser, so the region is consumed into `HeaderValues` eagerly at construction; `HeaderValues::new` is O(1), so this costs nothing and simplifies the struct to owning `values: HeaderValues<'a>` directly. (2) Entry point is `Deserializer::from_header_values(HeaderValues<'a>)`. (3) Transient dead-code warnings on the struct/fields remain until sub-items 2–3 and Phase 1b reference them.
- [x] Borrowing helper: move a `Value` out of the iterator/lookahead and coerce its slice(s) to shared `&'a` views (UTF-8 validation where a string is required), emitting the new error type's UTF-8 failure on invalid input. Note the forward-only invariant at this site.
  - Implemented as: `Deserializer::next_value()` (lookahead-aware pull, `Result<Option<Value<'a>>, Error>`), free fns `shared_bytes(&mut [u8]) -> &[u8]` and `shared_str(&mut [u8]) -> Result<&str, Error>` (UTF-8 → `ErrorParsing::InvalidUtf8`). Forward-only invariant noted at the coercion site: taking the mutable slice **by value** is what freezes the region.
  - Findings: (1) `HeaderValues::next()` yields `error::ErrorValue`, not `error::Error` — added a second `From<crate::error::ErrorValue>` impl routing through the existing `Parser(crate::error::Error)` wrapper (via the pre-existing `From<ErrorValue> for error::Error`), keeping all parser failures under one variant. (2) Both error enums now derive `Clone, Debug, PartialEq, Eq` (matches `error.rs` house style); `PartialEq` is required to `assert_eq!` full `Result`s in tests. (3) The parser's element validation accepts `obs-text` (0x80-FF), so non-UTF-8 elements DO reach the deserializer layer — `InvalidUtf8` is reachable through the public API and is tested end-to-end (`Example-X: te\xffxt`). (4) Because coercion consumes the mutable borrow, an element can be passed to only one helper per test; each helper has its own input line in tests.
- [x] Shape classification via the two-call pattern (head + discriminator, then drain loop); element-as-boundary item grouping.
  - Implemented as: `Shape` enum (`Empty`/`Single`/`List`) + `Deserializer::classify_shape()` (pulls head + discriminator, buffers both, returns shape) + `starts_new_item(&Value)` (element-as-boundary rule). The struct now carries **two** buffer slots — `head` and `lookahead` — because classification pulls two values; `next_value()` serves them in pull order (head → lookahead → iterator).
  - Findings: (1) a single element+params value without a comma (e.g. `a;b=1`) classifies as `List` under the literal two-call pattern (a continuation exists); distinguishing it from a true list would require draining until the next `Element`, which cannot be buffered in the fixed two-slot buffer for multi-parameter items — so hints resolve the ambiguity during their drain loops via the item-boundary discipline (Phase 1b). (2) Head kind (Element vs Parameter), read off the buffered head, distinguishes element-bearing from params-only shapes per the locked design. (3) `in_item_scope` remains unwritten-until-read until Phase 1c's `SeqAccess` sets it.

### 1b. Hints (no identifier matching yet)
- [x] `deserialize_str` / `deserialize_string` / `deserialize_bytes` / `deserialize_byte_buf`: borrowed single bare element; owned rejoin for bare-element lists; hard error when any parameter is present in scope.
  - Implemented as: full `impl<'de> de::Deserializer<'de> for &mut Deserializer<'de>` skeleton (all 31 trait methods) — the four string/bytes hints are real, every other hint returns `ErrorParsing::UnsupportedHint` via a local `unsupported_hint!` macro until its own sub-item swaps in the logic (keeps each commit green). Shared drain: `bare_element_scope()` (Empty → error; Parameter head → error; Single → `BareScope::Borrowed`; List → owned `", "`-rejoin, any `Parameter` mid-drain → error) + `visit_bare_element_as_string/bytes` emitters.
  - Findings: (1) the workspace builds serde with `default-features = false`, so `Visitor::visit_string`/`visit_byte_buf` and the `Deserialize` impls for `String`/`Vec<u8>` are **feature-gated behind std/alloc** — lib code therefore hands owned rejoin data over via `visit_str(&s)` / `visit_bytes(&v)` (the no-alloc-compatible path, same as serde_json's no_std build; the visitor copies), and `std` was added to the crate's **dev**-dependency features so tests can use owned targets. (2) serde deserializes `Vec<u8>` as a *sequence*, not via `deserialize_byte_buf` — the byte_buf hint is tested with a small manual `ByteBuf` target type that requests it explicitly. (3) Re-classification after `classify_shape()` + `visit_some(self)` (next sub-item) is idempotent because both pulled values are buffered and re-served in pull order.
- [x] `deserialize_option`: peek — empty → `visit_none`, else `visit_some(self)`.
  - Findings: the peek is exactly `classify_shape()` (which only buffers, never consumes), so `visit_some(self)` re-serves the identical stream — verified end-to-end by the list-rejoin case (`Option<String>` over `a, b` → `Some("a, b")` through option → inner string hint → second classification). A lone leading `;` parses to zero values (like an empty field value) and yields `None`.
- [x] Numeric hints (`i8..i64`, `u8..u64`): single bare element, strict digit parsing; error otherwise.
  - Implemented as: byte-oriented `parse_integer(bytes, signed) -> Result<i128, Error>` (ASCII digits only, optional leading `-` for signed, no `+`/whitespace; 19-digit guard keeps the i128 accumulation overflow-free — no str round-trip needed) + an `integer_hint!` macro expanding all eight trait methods (mirrors serde's own `impl_deserialize_num!` pattern). Width downcast via `TryFrom<i128>`; out-of-width and non-digit inputs → `ErrorParsing::InvalidInteger { found }`; list scope → `UnsupportedShape("expected a single bare element")`. `i128`/`u128` stay rejected per the plan.
- [x] `deserialize_seq` / `deserialize_tuple` / `deserialize_tuple_struct`: 1-element leniency for bare values; per-item grouping; params-only → 1-element seq of the flattened map.
  - Implemented as: `classify_shape()` at entry + `visitor.visit_seq(SeqAccess { de, len, seen })`; tuple/tuple_struct pass `len` (advisory stop), plain seq passes `None`. **Note:** this required Phase 1c's `SeqAccess` sub-item as a hard dependency — it was implemented here and is marked done below.
  - Findings: (1) item-boundary discipline lives in `Deserializer::next_value_in_scope()`: while `in_item_scope`, an `Element` is buffered back into `head` and reported as end-of-drain, so each item's drain always terminates either at stream end or with the next item's head buffered — which makes "an item exists" decidable as *any buffer non-empty* in `next_element_seed`. (2) Single-element items stay zero-copy: when no continuation is consumed within an item the rejoin buffer stays empty and the element is emitted borrowed (`Vec<String>` over `a, b` → two borrowed visits, no allocation). (3) The params-only clause ("1-element seq of the flattened map") is verified in the `map` sub-item, where `MapAccess` lands.
- [x] `deserialize_map` (params-only path): drain parameter pairs into a map; list of params-only items flattens; an `Element` mid-drain (mixed list) → hard error.
  - Implemented as: `classify_shape()` + head-kind gate (`Parameter` head required — element heads, bare or parameterized, are a name not key-value data → hard error; empty → zero-entry map) + `visitor.visit_map(MapAccess)`. **Note:** required Phase 1c's `MapAccess` sub-item (marked done below), plus a new `SliceDeserializer` over a single borrowed `&[u8]` so each parameter's key/value halves can be served to seeds.
  - Findings: (1) `SliceDeserializer` implements only str/string/bytes/byte_buf in v1 (borrowed, UTF-8 validated); all other hints are `UnsupportedHint` — e.g. numeric map keys are not supported yet. (2) The mid-drain `Element` case is handled by the shared `next_value_in_scope()`: hard error at value position (mixed list), clean item boundary under seq — one mechanism, both behaviors. (3) The deferred seq params-only clause from the previous step is verified here: `Vec<HashMap>` over `;a=1, ;b=2` → one 1-element seq holding a single flattened map.
- [x] `deserialize_any`: bare→str, list→seq, params-only→map, element+params→hard error, empty→unit.
  - Implemented as a dispatch on the buffered `(head, discriminator)` pair after `classify_shape()`: `(Element, None)` → borrowed str; `(Element, Parameter)` → hard error (`"element with parameters"`); `(Element, Element)` → seq at value position; `(Parameter, _)` → map (flattened params-only scope); empty head → unit.
  - Findings: (1) **critical item-scope nuance**: inside an item, `(Element, Some(Element))` means "bare element + next item's head (boundary)", not a list continuation — dispatching it to seq would re-classify the same two buffered values forever (infinite nesting). Under `in_item_scope` that arm emits the single bare element as str, leaving the boundary element buffered for the next seed. (2) Value-position lists whose later items carry parameters (`a, b;c=1`) correctly error from within the seq (item-level `element with parameters`). (3) Tested via a hand-written `AnyValue` dynamic target (unit/str/seq/map visitor), since no built-in serde type drives `deserialize_any`.
- [x] `deserialize_ignored_any`: drain without visiting.
  - Implemented as: main deserializer consumes the head value explicitly, then drains its continuation via `next_value_in_scope()` (item-scope-aware) and completes with `visitor.visit_unit()`; `SliceDeserializer` completes in place (a slice is a single value).
  - Findings: (1) serde's `Visitor` has **no `visit_ignored_any`** method — `IgnoredAny` is itself a lenient visitor, so the contract for `deserialize_ignored_any` is "advance past the value, then complete with some visit call"; `visit_unit()` is used. (2) **Infinite-loop bug caught by tests**: draining via `next_value_in_scope()` alone does not make forward progress in item scope — the first pulled value *is* the current item's head element, which `next_value_in_scope` re-buffers as a "boundary", so `Vec<IgnoredAny>` re-serves the same item forever. Fix: consume the head explicitly before draining; each in-scope call now discards ≥1 value. (3) Params-only items have no element boundary, so an ignored item over `;a=1, ;b=2` consumes both (consistent with the documented params-only flattening limitation).
- [x] v1-rejected hints (bool, floats, anything else not implemented): clear unsupported error via `ErrorParsing`.
  - Verified: bool, f32, f64, char, i128, u128, unit, unit_struct, newtype_struct all return `ErrorParsing::UnsupportedHint(<hint name>)` from the main deserializer (the `unsupported_hint!` placeholders written in sub-item 1 stay as-is).
  - Findings: (1) rejection happens at the hint level, before any content inspection — tested with plausible-looking inputs (`true`, `1.5`, `a`) to pin that down. (2) struct/enum/identifier remain `UnsupportedHint` until Phases 2–3 replace them; the same placeholder pattern in `SliceDeserializer` covers its full non-str/bytes hint set.

### 1c. Access objects
- [x] `SeqAccess` over `&mut D`; sets/clears the item-scope flag around each element seed call. *(Pulled forward by the seq hints, which cannot be expressed without it — implemented in that step.)*
- [x] `MapAccess` over `&mut D` for parameter key→value pairs (keys emitted raw for now; identifier matching wires in at Phase 2). *(Pulled forward by the map hint, which cannot be expressed without it — implemented in that step.)*

## Phase 2 — Identifier matching machinery

> Its own phase, directly after the core body and before struct/enum, which build on it.

- [x] Implement `normalize(bytes)` = strip non-alphanumerics + lowercase. Unit-test the bridging cases: `text/plain`↔`TextPlain`, `max-age`↔`max_age`, `content-type`↔`content_type`, plus a collision case.
  - Implemented as a byte-level free fn returning an owned `Vec<u8>` (ASCII-only semantics: `is_ascii_alphanumeric` filter + `to_ascii_lowercase`; non-ASCII/obs-text bytes are stripped, so matching is defined for non-UTF-8 wire bytes too).
  - Findings: the owned return is internal scratch — it never reaches a visitor; matching in the next sub-item allocates one buffer per identifier plus one per expected name (bounded by field count), noted there.
- [x] Add a **pending expected-name list** slot on the deserializer: stores `&'static [&'static str]`, set by `deserialize_struct`/`deserialize_enum`, consumed by `deserialize_identifier`.
  - Implemented as `Deserializer::pending_names: Option<&'static [&'static str]>`, initialized to `None` in `from_header_values`. A roundtrip test pins the initial state and the exact type (set/read via direct field access, as tests already do for the buffer slots).
- [x] Implement `deserialize_identifier`:
  - With an expected-name list present: first-normalized-match wins → feed the exact expected name; otherwise feed raw wire bytes (rename/alias fallback).
  - With no list (e.g. `HashMap` keys): feed raw wire bytes.
  - Implemented as: main deserializer pulls the current value (Element → element bytes, Parameter → key half), then a shared free fn `match_identifier(wire, names) -> Option<&'static str>` does first-normalized-match-wins; a match feeds the exact expected name via `visit_borrowed_str`, otherwise the raw wire bytes are UTF-8-validated and fed. Empty stream → `UnsupportedShape("expected an identifier")`.
  - Findings: (1) matching is byte-level, so non-UTF-8 wire bytes can still match an expected name — only the fallback path requires valid UTF-8 (`InvalidUtf8`). (2) Phase 3's access objects will need to match already-pulled bytes without re-pulling from the stream; `match_identifier` + a small identifier-serving deserializer over raw bytes is the intended wiring (decided when MapAccess/EnumAccess land).
- [x] Unit tests: normalized hit, collision first-wins, rename fallback, no-list passthrough.
  - Implemented via an `Identifier` capture target driving `deserialize_identifier` directly; plus a parameter-key-half test pinning that a `Parameter` contributes its key (not value) as the identifier.

## Phase 3 — Struct + Enum deserialization (build on identifier matching)

- [x] `deserialize_struct`: single item only; read expected name + `fields`; validate the element (bare or +params) against the struct name via `normalize` (params-only skips the check; mismatch → `ErrorParsing`); set the pending-name slot to `fields`; `visit_map(MapAccess)` streaming parameter key→value with keys identifier-matched against fields. Bare element → zero-entry map.
  - Implemented as: classify → Empty error → consume head (Element → normalize-validate against `name`, mismatch → `NameMismatch { expected, found }`; Parameter → re-buffered into `head` so the map serves it) → save/restore `pending_names` (= `fields`) and `in_item_scope` (set true so the map drain stops at the next item boundary) → `visit_map(MapAccess)` → single-item check: at value position a remaining buffered value is `UnsupportedShape("expected a single item")`; inside a seq item the trailing boundary belongs to the next seed.
  - Findings: (1) `NameMismatch.expected` changed from `&'static str` to `String` — serde hands the struct name out as a plain `&str`, which cannot be stored with a `'static` bound (allocation is error-path only). (2) Key routing: `SliceDeserializer` gained a `names: Option<&'static [&'static str]>` field and a real `deserialize_identifier` (shared `match_identifier`), so `MapAccess` serves keys identifier-matched when the pending list is set (struct/enum-variant fields) and raw for plain maps — one construction site, mode controlled by the slot. The main deserializer's `deserialize_identifier` now delegates to the same `SliceDeserializer`. (3) `SliceDeserializer` also gained `option` (always `visit_some` — absence is the missing-key case resolved by serde's map semantics), the numeric hints via a `slice_integer_hint!` macro, and `unit`, since struct field values are arbitrary types. (4) **serde default**: without `deny_unknown_fields`, derived structs silently ignore unknown keys (generated `__other` catch-all + wildcard match arm) — both that and the `deny_unknown_fields` error path are tested.
- [x] `deserialize_enum` (externally tagged only): single item only; set pending-name slot to `variants`; `visit_enum(EnumAccess)`.
  - Implemented as: classify → Empty error → save/restore `pending_names` (= `variants`, kept for the whole enum since every item re-matches a variant) → `visit_enum(EnumAccess)` → single-item check (value position only, mirroring `deserialize_struct`).
  - [x] `EnumAccess::variant_seed`: head must be an element (params-only → `MissingElement`); identifier-match it against variants.
    - Implemented as: pull head (Empty → shape error; Parameter → `MissingElement`) → serve its bytes through `SliceDeserializer { names: pending_names }` to the seed — reusing the exact identifier-matching path of struct field keys.
  - [x] `VariantAccess`: unit variant (bare element, nothing more); newtype + struct variants stream the item's parameters as a map (reuse `MapAccess`).
    - Implemented as: `unit_variant` pulls one value under item scope (Parameter → `UnsupportedShape("expected a bare element, found parameters")`; Element/None → Ok — the boundary is buffered back for the next seed); `newtype_variant_seed` forwards to the main deserializer, which sees a params-only scope (element already consumed as the variant name) and skips the name check; `struct_variant` saves/restores both slots and streams through the shared `MapAccess`; `tuple_variant` stays rejected (`UnsupportedHint("tuple variant")`) — positional payloads have no v1 representation.
  - Findings: (1) the variant-name list must persist across items (not cleared per variant) — restored after `visit_enum` returns; `struct_variant` temporarily overrides it with the variant's fields, then restores the variants so following items keep matching. (2) `unit_variant` sets item scope itself so a trailing boundary element is buffered back (and caught by the single-item check) instead of being silently consumed at value position.
- [x] Test: a bare and a parameterized occurrence of the same element both land in one enum variant (Option-field discipline) — confirms mixed bare/parameterized lists deserialize into a single enum.
  - `Vec<MimeItem>` over `text/plain, text/plain; charset=utf-8` → `[TextPlain { None }, TextPlain { Some("utf-8") }]`: the item-boundary discipline (element buffered back per seed) plus the zero-entry-map behavior of a bare occurrence make mixed lists land in one variant, with the `Option` field carrying the distinction.

## Phase 4 — Section-level deserializer

- [x] Section deserializer wrapping `Headers<'a>` (the line iterator), presenting a map of name → value.
  - Implemented as `SectionDeserializer { headers: Headers<'a> }` with `from_headers()` ctor and a full trait impl on `&mut Self`: `deserialize_map`/`deserialize_struct` are real (streaming via `SectionMapAccess`); every other hint is `unsupported_hint!` (a section is not a scalar, list, or enum).
- [x] Its `MapAccess`: keys = lower-cased header names, identifier-matched against the target's struct fields; each value delegates to a fresh value-level deserializer for that line.
  - Implemented as `SectionMapAccess { headers: &mut Headers<'de>, names, pending_values }`: `next_key_seed` pulls one line, serves its lower-cased name through `SliceDeserializer { names }` (identifier-matched for struct targets via the slot, raw for plain maps — same slot-control as value-level maps); `next_value_seed` hands the line's `HeaderValues` to a fresh value-level `Deserializer::from_header_values`.
  - Findings: (1) **Soundness**: each parsed line is a *disjoint slice* of the input buffer (`Headers::next` uses `split_at_mut`), so holding a line's `HeaderValues` while continuing to iterate `Headers` never aliases a mutable region — no copy, no index tracking needed. (2) Duplicate field names are plain serde semantics, verified by tests rather than implemented: maps last-wins, derived structs error with "duplicate field". (3) Parser errors surface through the `Parser` wrapper unchanged (tested: an illegal field-name byte yields `Error::Parser(InvalidFieldName)`).
- [x] Public entry point to deserialize a whole header section into a typed struct or a map. (Duplicate-name behavior is plain serde semantics — derived structs error, maps last-wins — nothing to implement.)
  - Implemented as `pub mod de` in lib.rs: re-exports `Error`/`ErrorParsing` (now `pub`, with the pre-existing `std::error::Error` impls) and provides `from_headers<'a, T>(headers: line::Headers<'a>) -> Result<T, Error> where T: Deserialize<'a>`, which drives a `SectionDeserializer`. The module is deliberately separate from the user-owned `prelude`.
  - Findings: (1) `Error`/`ErrorParsing` moved from `pub(crate)` to `pub` inside the still-private `deserializer` module and re-exported through `de` — public path is `http_headers::de::{Error, ErrorParsing}`; no other public surface changed. (2) Verified through the public API: struct target, plain map target, and a parser error surfacing as `de::Error::Parser(InvalidFieldName)` (3 tests).

## Phase 5 — Test hardening & edge cases

- [x] Table-driven tests across the full hint × shape matrix (every cell in the spec).
  - Implemented as 17 table tests over six canonical shapes (empty, bare element, bare-element list, params-only, element+params, mixed list) × ten hint families (str, bytes, unsigned/signed integer, seq, tuple, map, struct, enum, option, any, ignored_any, plus the nine v1-rejected hints × all shapes), driven by a `check_matrix_cell` helper that compares full Ok/Err outcomes. Borrowing modes pinned separately: single bare element → `visit_borrowed_*`, list → owned `", "`-rejoin via `visit_*`.
  - Findings: (1) serde 1.0.229 has **no specific `Cow<'de, str>` / `Cow<'de, [u8]>` Deserialize impls** — only the generic fallback (`T::Owned::deserialize`, always owned; `Vec<u8>` deserializes as a seq of u8s) — so borrowing-mode checks need manual capture visitors (`StrEmission`/`BytesEmission`) recording which visit method fired. (2) serde's own `de::Value` cannot serve as the any/seq matrix target: its map visitor deserializes keys via `deserialize_any`, which `SliceDeserializer` rejects — the pre-existing `AnyValue` capture type (String keys) is used instead. (3) Tuple family over short streams surfaces serde's `invalid_length` as a `Custom` error — plain serde semantics, asserted by message content.
- [x] Error-path tests: each `ErrorParsing` failure, plus parser errors surfacing through the `From<error::Error>` wrapper.
  - 14 new `error_path_*` tests complete the coverage: `InvalidUtf8` (str path over an `obs-text` byte; identifier rename/alias fallback), `UnsupportedShape("expected an identifier")` (empty stream), and the two defensive ordering errors (`MapAccess::next_value_seed` / `SectionMapAccess::next_value_seed` called before a key/name — pinned via direct object construction since serde's flow can't reach them). Parser wrapper: line-level `MissingCRLFDelimiter`/`MissingFieldDelimiter`/`MissingFieldName` at section level, and all six `ErrorValue` variants (`EmptyList`, `MissingClosingQuote`, `LeftoverBytes`, `MissingParameterDelimiter`, `MissingParameterKey`, `MissingParameterValue`) at value level using the exact wire forms from `error.rs`'s doc examples.
  - Findings: (1) every `ErrorParsing` variant now has an end-to-end trigger (several were already covered by earlier phases: `MissingElement`/`NameMismatch` in Phase 3, `UnsupportedShape`/`UnsupportedHint`/`InvalidInteger`/`Custom` via the matrix and serde-semantics tests). (2) `error::Error::EmptyLine` is **unreachable through deserialization** — `Headers` terminates on the first bare CRLF before an empty line can be produced; only reachable by calling `HeaderLine::new` directly. (3) The defensive ordering checks required importing the `MapAccess` trait into the test module (`use de::MapAccess as _;`) plus `std::marker::PhantomData` (not in the std prelude) to drive seeds directly.
- [x] Forward-only / borrowing invariant check (assert no region is mutated after being yielded; a targeted runtime or compile-time guard as appropriate).
  - Two guards: (1) **runtime** — `forward_only_yielded_regions_stay_frozen` deserializes `Vec<FrozenItem<'_>>` (each item holds a *borrowed* `&str` parameter value) from a two-item list where item 2's key (`Note`) and quoted value are mutated in place by the parser *after* item 1's region was yielded; item 1's borrowed view stays byte-identical to the wire while item 2 shows its own in-place mutation (lower-cased key, decoded quoted pair). (2) **compile-time** — `forward_only_shared_coercion_moves_mutable_borrow` pins that `shared_bytes` takes the `&mut [u8]` by value, so no mutable handle survives after a region is shared.
  - Findings: (1) The runtime test *compiling* is itself the soundness proof: the borrow checker accepts holding cross-item shared borrows alongside the deserializer's live mutable state, confirming the disjoint-slice model (each parsed line/value is a `split_at_mut` slice) leaves no aliasing between yielded regions and later parser work. (2) First draft failed with `MissingClosingQuote`: a raw comma inside a quoted parameter value of an *unquoted* list item breaks list-state splitting — the parser's list state is quote-aware only when the element starts with `"`. Switched to the escaped-quote case (`"bet\"a"` → `bet"a`), which exercises `decode_quoted_pairs` without hitting that limitation.
- [ ] Clippy pedantic clean under workspace lints (no unwrap/expect/shadowing/prints).
- [ ] (Stretch) Bolero fuzz target: random header bytes → permissive target, assert it always returns a consistent `Ok`/`Err` and never panics.

---

## Out of scope (owned separately)
- Documentation, prelude exports, and comment polish — written on top of this implementation as a starting point.

## Definition of done
- Every matrix cell behaves per the spec; the full test suite (existing + new) is green.
- Clippy pedantic clean under workspace lints.
- Existing public API behavior is unchanged (all pre-existing tests pass untouched); `error.rs` unmodified.
- Identifier matching is integrated throughout (section names, variants, parameter keys), not bolted on.
