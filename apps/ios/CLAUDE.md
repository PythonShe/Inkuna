# Inkuna iOS

| Layer | Choice | Notes |
|---|---|---|
| UI | UIKit, programmatic layout | no storyboards or Interface Builder artifacts |
| Project | XcodeGen | `project.yml` is the source of truth; `Inkuna.xcodeproj` is generated |
| Language | Swift 6 language mode (strict concurrency) | latest Xcode/SDK |
| Core | `InkunaCore.xcframework` + `Generated/InkunaCore.swift` | built by `../../scripts/build-core-ios.sh`; core methods are `async` |
| Rendering | `Reader/Engine/PageView.swift` | draws core display-list glyph runs via `CTFontDrawGlyphs` |

```bash
../../scripts/build-core-ios.sh   # after any core/FFI change
xcodegen generate                 # after any project.yml change
xcodebuild -project Inkuna.xcodeproj -scheme Inkuna \
  -destination 'generic/platform=iOS Simulator' build   # must pass
```

## Architecture

- Thin shell: all domain logic goes through the Rust core
  (`try await LibraryStore.shared.library()`, a `Bookshelf`). Never duplicate
  parsing, DB access, or progress math in Swift.
- The generated bindings are compiled into the app target: do not
  `import InkunaCore`; the types are directly visible.
- One feature folder per area under `Inkuna/` (`Library/`, `Reader/`,
  `Settings/`, ...), one primary view controller per screen.

## Conventions

- Modern APIs at the iOS 18 baseline: `UIContentConfiguration`, diffable data
  sources for nontrivial lists, `UIButton.Configuration`. Manual layout is
  fine in performance-critical reader views.
- Liquid Glass only behind `if #available(iOS 26.0, *)` with a material
  fallback; never raise the deployment target for it.
- Support Dynamic Type and light/dark appearance.
- Feel matches Apple Books: interruptible transitions, spring physics,
  haptics.
- User-visible strings live in `Inkuna/Localizable.xcstrings` (16
  languages); never hard-code a user-facing literal. Resolve strings at
  view-controller level.
