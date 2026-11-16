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
  - An **`ErrorParsing`** variant grouping the deserialization-specific failures: `MissingElement`, element/name mismatch, invalid UTF-8, unsupported shape (parameterized value where a bare element is required; `any` × element+params), unsupported hint (bool/float in v1).
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
- Zero-copy except the one documented allocation (owned rejoin for str/bytes over bare-element lists).
- **`line.rs` encapsulation**: no refactors exposing parser internals; the deserializer uses only the public API of the existing types.
- The existing public API keeps **exactly** its current observable behavior — all existing tests in `line.rs`/`stack.rs` stay green.
- No doc/comment polish is required; the implementation is a starting point.

---

## Phase 0 — Foundations & scaffolding

- [ ] Add `mod deserializer;` to `src/lib.rs` (visibility settled later alongside the prelude work).
- [ ] New standalone error type per the Errors section: parser-error wrapper variant + `From<error::Error>`; `ErrorParsing` grouping the serde-specific failures; `Display`/`Debug`/`std::error::Error`/`serde::de::Error` impls. **No changes to `error.rs`.**
- [ ] Confirm `serde` is a normal dependency (it is: `serde.workspace = true`); `derive` stays dev-only. No `Cargo.toml` change expected — verify.

## Phase 1 — Value-level deserializer

### 1a. Skeleton, lookahead discipline, borrowing mechanics
- [ ] Deserializer struct owning: the field-value region (lazily initialized into a `HeaderValues<'a>`), one lookahead `Option<Value<'a>>`, and an item-scope flag. Entry point following serde's `from_*` convention.
- [ ] Borrowing helper: move a `Value` out of the iterator/lookahead and coerce its slice(s) to shared `&'a` views (UTF-8 validation where a string is required), emitting the new error type's UTF-8 failure on invalid input. Note the forward-only invariant at this site.
- [ ] Shape classification via the two-call pattern (head + discriminator, then drain loop); element-as-boundary item grouping.

### 1b. Hints (no identifier matching yet)
- [ ] `deserialize_str` / `deserialize_string` / `deserialize_bytes` / `deserialize_byte_buf`: borrowed single bare element; owned rejoin for bare-element lists; hard error when any parameter is present in scope.
- [ ] `deserialize_option`: peek — empty → `visit_none`, else `visit_some(self)`.
- [ ] Numeric hints (`i8..i64`, `u8..u64`): single bare element, strict digit parsing; error otherwise.
- [ ] `deserialize_seq` / `deserialize_tuple` / `deserialize_tuple_struct`: 1-element leniency for bare values; per-item grouping; params-only → 1-element seq of the flattened map.
- [ ] `deserialize_map` (params-only path): drain parameter pairs into a map; list of params-only items flattens; an `Element` mid-drain (mixed list) → hard error.
- [ ] `deserialize_any`: bare→str, list→seq, params-only→map, element+params→hard error, empty→unit.
- [ ] `deserialize_ignored_any`: drain without visiting.
- [ ] v1-rejected hints (bool, floats, anything else not implemented): clear unsupported error via `ErrorParsing`.

### 1c. Access objects
- [ ] `SeqAccess` over `&mut D`; sets/clears the item-scope flag around each element seed call.
- [ ] `MapAccess` over `&mut D` for parameter key→value pairs (keys emitted raw for now; identifier matching wires in at Phase 2).

## Phase 2 — Identifier matching machinery

> Its own phase, directly after the core body and before struct/enum, which build on it.

- [ ] Implement `normalize(bytes)` = strip non-alphanumerics + lowercase. Unit-test the bridging cases: `text/plain`↔`TextPlain`, `max-age`↔`max_age`, `content-type`↔`content_type`, plus a collision case.
- [ ] Add a **pending expected-name list** slot on the deserializer: stores `&'static [&'static str]`, set by `deserialize_struct`/`deserialize_enum`, consumed by `deserialize_identifier`.
- [ ] Implement `deserialize_identifier`:
  - With an expected-name list present: first-normalized-match wins → feed the exact expected name; otherwise feed raw wire bytes (rename/alias fallback).
  - With no list (e.g. `HashMap` keys): feed raw wire bytes.
- [ ] Unit tests: normalized hit, collision first-wins, rename fallback, no-list passthrough.

## Phase 3 — Struct + Enum deserialization (build on identifier matching)

- [ ] `deserialize_struct`: single item only; read expected name + `fields`; validate the element (bare or +params) against the struct name via `normalize` (params-only skips the check; mismatch → `ErrorParsing`); set the pending-name slot to `fields`; `visit_map(MapAccess)` streaming parameter key→value with keys identifier-matched against fields. Bare element → zero-entry map.
- [ ] `deserialize_enum` (externally tagged only): single item only; set pending-name slot to `variants`; `visit_enum(EnumAccess)`.
  - [ ] `EnumAccess::variant_seed`: head must be an element (params-only → `MissingElement`); identifier-match it against variants.
  - [ ] `VariantAccess`: unit variant (bare element, nothing more); newtype + struct variants stream the item's parameters as a map (reuse `MapAccess`).
- [ ] Test: a bare and a parameterized occurrence of the same element both land in one enum variant (Option-field discipline) — confirms mixed bare/parameterized lists deserialize into a single enum.

## Phase 4 — Section-level deserializer

- [ ] Section deserializer wrapping `Headers<'a>` (the line iterator), presenting a map of name → value.
- [ ] Its `MapAccess`: keys = lower-cased header names, identifier-matched against the target's struct fields; each value delegates to a fresh value-level deserializer for that line.
- [ ] Public entry point to deserialize a whole header section into a typed struct or a map. (Duplicate-name behavior is plain serde semantics — derived structs error, maps last-wins — nothing to implement.)

## Phase 5 — Test hardening & edge cases

- [ ] Table-driven tests across the full hint × shape matrix (every cell in the spec).
- [ ] Error-path tests: each `ErrorParsing` failure, plus parser errors surfacing through the `From<error::Error>` wrapper.
- [ ] Forward-only / borrowing invariant check (assert no region is mutated after being yielded; a targeted runtime or compile-time guard as appropriate).
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
