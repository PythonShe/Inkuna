# Inkuna iOS Rules

You are working inside `apps/ios/`, the native iOS shell. Follow the rules
below; they capture project-specific conventions and override common defaults.

## 0. Tech Stack & Dev Commands

| Layer | Technology | Notes |
|------|------|------|
| UI | **UIKit** (owner preference — no SwiftUI) | view controllers + programmatic layout, no storyboards |
| Project | XcodeGen | `project.yml` is the source of truth; `Inkuna.xcodeproj` is generated and gitignored |
| Language | Swift, **Swift 6 language mode** (strict concurrency) | latest Xcode/SDK |
| Core | `InkunaCore.xcframework` + generated Swift bindings | produced by `../../scripts/build-core-ios.sh`; core methods are `async` |
| Rendering (planned) | Readium Swift Toolkit | `EPUBNavigatorViewController` is UIKit-native |
| Targets | min iOS 18, built with latest SDK | bundle id `app.inkuna.ios` |

```bash
../../scripts/build-core-ios.sh   # after any core/FFI change
xcodegen generate                 # after any project.yml change
xcodebuild -project Inkuna.xcodeproj -scheme Inkuna \
  -destination 'generic/platform=iOS Simulator' build   # must pass
```

## 1. Architecture

- The shell is thin: all domain logic goes through the Rust core
  (`LibraryStore.shared.library`, a `Bookshelf`). Never duplicate core logic
  (parsing, DB access, progress math) in Swift.
- The generated bindings (`Generated/InkunaCore.swift`) are compiled into the
  app target — do **not** `import InkunaCore` in app code; types are directly
  visible. Never edit generated files.
- Feature folders under `Inkuna/` (e.g. `Library/`, later `Reader/`,
  `Settings/`), one primary view controller per screen.

## 2. UIKit Conventions

- Programmatic Auto Layout (or manual layout in performance-critical reader
  views); no Interface Builder artifacts.
- Modern APIs at min-18 baseline: `UIContentConfiguration`, diffable data
  sources for nontrivial lists, `UIButton.Configuration`.
- iOS 26 Liquid Glass adoption is welcome but always gated:
  `if #available(iOS 26.0, *)` with a dignified material fallback — never
  raise the deployment target for it.
- Respect Dynamic Type and dark mode from the first commit; the reader's
  ink/moonlight themes come later, system appearance correctness comes now.

## 3. Quality Bar

- Apple Books is the reference for feel: interruptible transitions, spring
  physics, haptics. Nothing user-facing ships janky-but-works.
- User-visible strings will be localized (CJK audiences are primary); avoid
  burying literals deep in code — keep them at view-controller level so the
  l10n pass stays mechanical.
