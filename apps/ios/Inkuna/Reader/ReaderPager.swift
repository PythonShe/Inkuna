import UIKit

/// The reader's own pager: every horizontal page turn — within a resource
/// and across resource (chapter) boundaries — is driven here, on our
/// gesture pipeline and our physics, instead of the renderer's. The
/// renderer keeps rendering, preloading, and bookkeeping; its native
/// horizontal gestures are suppressed through the surface, so a boundary
/// crossing tracks the finger and settles exactly like an inner turn.
///
/// The model is one continuous strip. A drag accumulates into a raw strip
/// coordinate; the resource's inner pages consume it first, and whatever
/// overflows a clamped inner edge spills into the outer resource strip —
/// which is how a single gesture flows from the last page of a chapter
/// straight into dragging the next chapter in, both live under the
/// finger. Reversing unwinds in the same order. Purely geometric: RTL and
/// vertical-writing publications page along the same axes, so no
/// progression-aware branch exists below the "forward means what?"
/// mapping for taps and keys.
///
/// Release physics and thresholds live in `ReaderPagerRules`, shared with
/// the Android shell. Every animation is a critically damped spring fed
/// the finger's release velocity, stepped on a display link (ProMotion-
/// native), and interruptible: a touch freezes it in place, a drag from
/// there adopts the frozen position into a live gesture, and a bare tap
/// lets the frozen turn finish rather than re-deciding it.
///
/// A boundary crossing that settles on the neighbor's slot is then
/// committed through the surface, which updates the renderer's
/// bookkeeping without moving anything on screen.
@MainActor
final class ReaderPager: NSObject, UIGestureRecognizerDelegate {
    private let surface: ReaderPagerSurface
    private weak var hostView: UIView?

    private weak var pan: UIPanGestureRecognizer?
    private weak var touchObserver: UILongPressGestureRecognizer?

    // MARK: Strip state (one interaction)

    /// Raw, unresisted strip coordinate — the finger's accumulated
    /// content travel. Inner range consumes it; overflow beyond an inner
    /// bound is boundary travel.
    private var stripRaw: CGFloat = 0
    private var innerRange: ClosedRange<CGFloat> = 0 ... 0
    private var pageWidth: CGFloat = 0
    /// The committed outer origin the interaction started from. Boundary
    /// travel is displayed relative to it and it only moves through a
    /// commit.
    private var outerHome: CGFloat = 0
    private var outerRange: ClosedRange<CGFloat> = 0 ... 0
    /// The inner page the gesture began on, for the commit rule.
    private var startPage = 0
    /// The inner bound the strip last exited through — which edge
    /// boundary travel is measured from when re-adopting an interrupted
    /// interaction.
    private var exitBound: CGFloat = 0
    /// Displayed outer displacement from home (rubber-band already
    /// applied). Nonzero means a boundary interaction is on screen.
    private var boundaryDisplacement: CGFloat = 0
    /// A strip has been captured and not yet released to rest.
    private var interactionActive = false
    /// The gesture began while the renderer was busy; baselines are
    /// captured on the first quiet frame so a flick landing right after
    /// a commit is honored instead of dropped.
    private var awaitingBaseline = false
    private var lastTranslationX: CGFloat = 0

    // MARK: Animation state

    private let spring = ReaderPagerSpring()
    private enum SpringRole {
        case inner
        case outerCommit(toRight: Bool)
        case outerReturn
    }
    private var springRole: SpringRole = .inner
    /// A spring frozen by a touch-down, resumed on a bare touch-up so an
    /// in-flight turn finishes instead of being re-decided; a drag
    /// adopts the frozen position into the live gesture instead.
    private struct FrozenSpring {
        var role: SpringRole
        var target: CGFloat
        /// Momentum at the freeze — a resume restarts with it, so a tap
        /// mid-turn doesn't stall the flight to a rest-start crawl.
        var velocity: CGFloat
    }
    private var frozen: FrozenSpring?
    /// Re-asserts the displaced outer offset every frame while a
    /// boundary interaction holds without a running spring: the
    /// renderer's layout pass snaps the outer strip back to its own
    /// index alignment whenever something (a landing preload) invalidates
    /// it, and per-frame re-writes make that at worst a one-frame flicker.
    private let holdLoop = ReaderPagerFrameLoop()
    /// The commit handed to the surface; gates fresh gestures for the
    /// few frames the renderer's bookkeeping needs.
    private var committing = false
    private var commitTask: Task<Void, Never>?

    private let boundaryHaptic = UIImpactFeedbackGenerator(style: .soft)

    /// Fired the moment turn intent shows — a claimed drag or an edge
    /// tap — so the chrome can clear before the motion, not after it.
    var onPageTurnGesture: (() -> Void)?

    init(surface: ReaderPagerSurface, view: UIView) {
        self.surface = surface
        hostView = view
        super.init()

        // Observes touch-down/up without ever claiming the touch: what
        // freezes and resumes springs.
        let touch = UILongPressGestureRecognizer(target: self, action: #selector(touched))
        touch.minimumPressDuration = 0
        touch.allowableMovement = .greatestFiniteMagnitude
        touch.cancelsTouchesInView = false
        touch.delegate = self
        view.addGestureRecognizer(touch)
        touchObserver = touch

        // The sole horizontal driver. It cancels the web content's
        // touches once it claims a drag, so nothing in the page competes
        // with a page turn; taps never reach the slop and pass through
        // untouched.
        let pan = UIPanGestureRecognizer(target: self, action: #selector(panned))
        pan.maximumNumberOfTouches = 1
        pan.delegate = self
        view.addGestureRecognizer(pan)
        self.pan = pan

        engageIfNeeded()
    }

    // MARK: Engagement

    /// Suppresses or restores the renderer's native gestures to match
    /// what is on screen. Called on init, on every navigation change
    /// (new preloaded views appear with native gestures enabled), after
    /// preference changes, and at gesture start.
    func engageIfNeeded() {
        if surface.isEngageable {
            surface.suppressNativeGestures()
            pan?.isEnabled = true
        } else {
            surface.restoreNativeGestures()
            pan?.isEnabled = false
        }
    }

    /// Cancels any live interaction and puts the strips back on their
    /// committed alignment — for rotation and teardown, where the
    /// renderer's own re-layout takes over.
    func cancelInteraction() {
        spring.cancel()
        holdLoop.stop()
        // A commit caught mid-flight is invalidated, not awaited: its
        // continuation must never write offsets or re-suppress gestures
        // over whatever re-layout triggered this cancel, and a commit
        // that never resolves must not leave `committing` blocking every
        // future turn.
        commitTask?.cancel()
        commitTask = nil
        committing = false
        frozen = nil
        awaitingBaseline = false
        if interactionActive {
            surface.setOuterOffset(outerHome)
            interactionActive = false
            boundaryDisplacement = 0
        }
    }

    // MARK: Programmatic turns (edge taps, keys)

    /// Turns toward +x — the geometric right, whatever the reading
    /// progression. Returns whether a turn was actually initiated: every
    /// entry point below can honestly decline (a busy renderer, a
    /// commit in flight, the end of the book), and callers that speak for
    /// the reader — VoiceOver's scroll action — must report that refusal
    /// rather than claim a page they never turned.
    @discardableResult
    func turnRight() -> Bool {
        turn(direction: 1)
    }

    /// Turns toward -x.
    @discardableResult
    func turnLeft() -> Bool {
        turn(direction: -1)
    }

    /// Turns toward the next page in reading order.
    @discardableResult
    func turnForward() -> Bool {
        turn(direction: surface.isRightToLeft ? -1 : 1)
    }

    /// Turns toward the previous page in reading order.
    @discardableResult
    func turnBackward() -> Bool {
        turn(direction: surface.isRightToLeft ? 1 : -1)
    }

    private func turn(direction: CGFloat) -> Bool {
        guard surface.isEngageable, !committing, !surface.isBusy else { return false }
        engageIfNeeded()
        onPageTurnGesture?()

        // A turn already in flight: successive taps chain by moving the
        // running spring's goal one page further, staying inside the
        // resource. A running boundary commit takes the tap as "seen" and
        // finishes.
        if spring.isRunning {
            switch springRole {
            case .inner:
                let next = spring.target + direction * pageWidth
                if next >= innerRange.lowerBound - 2, next <= innerRange.upperBound + 2 {
                    spring.retarget(min(max(next, innerRange.lowerBound), innerRange.upperBound))
                }
                return true
            case .outerReturn:
                // A return settle is purely cosmetic — it is travelling
                // back to the page already on screen. Swallowing the tap
                // would lose a real turn, so land the settle at its base
                // right now and fall through to perform the turn. Mirrors
                // the Android shell's F17 fix; commit flights below stay
                // "seen", because those are turns the reader has already
                // been shown.
                //
                // Viability comes first: the settle may only be destroyed
                // when a turn can take its place this instant. At a true
                // book boundary there is no neighbor to turn to, and
                // cancelling would snap `outerHome` into place with no
                // animation for nothing — so leave the settle running and
                // take the tap as "seen".
                guard returnAbortCanTurn(direction: direction) else { return true }
                spring.cancel()
                surface.setOuterOffset(outerHome)
                boundaryDisplacement = 0
                interactionActive = false
                updateHoldLoop()
            case .outerCommit:
                return true
            }
        }
        guard !interactionActive, frozen == nil else { return false }
        guard captureBaselines() else { return false }

        let innerTarget = stripRaw + direction * pageWidth
        if innerTarget >= innerRange.lowerBound - 2, innerTarget <= innerRange.upperBound + 2 {
            startInnerSpring(
                from: stripRaw,
                to: min(max(innerTarget, innerRange.lowerBound), innerRange.upperBound),
                velocity: 0
            )
        } else {
            // A boundary turn: reveal the neighbor with the same spring a
            // drag-release uses, then commit.
            guard neighborExists(direction: direction) else {
                interactionActive = false
                return false
            }
            exitBound = direction > 0 ? innerRange.upperBound : innerRange.lowerBound
            boundaryHaptic.prepare()
            startOuterSpring(
                from: outerHome,
                to: outerHome + direction * pageWidth,
                velocity: 0,
                role: .outerCommit(toRight: direction > 0)
            )
        }
        return true
    }

    // MARK: Gesture plumbing

    func gestureRecognizer(
        _ gestureRecognizer: UIGestureRecognizer,
        shouldRecognizeSimultaneouslyWith otherGestureRecognizer: UIGestureRecognizer
    ) -> Bool {
        true
    }

    func gestureRecognizerShouldBegin(_ gestureRecognizer: UIGestureRecognizer) -> Bool {
        guard gestureRecognizer === pan else { return true }
        // A commit in flight does not refuse recognition: the gesture is
        // funneled into the awaiting-baseline path below, so a flick
        // landing during the commit's bookkeeping window is honored
        // instead of silently dropped.
        guard surface.isEngageable else { return false }
        // Horizontal drags belong to the selection handles while text is
        // selected.
        guard !surface.hasActiveSelection else { return false }
        // A freshly preloaded spread arrives with its native pan enabled;
        // suppressing here — before recognition — keeps this very touch
        // from being claimed natively.
        surface.suppressNativeGestures()
        guard let view = gestureRecognizer.view else { return false }
        let velocity = pan?.velocity(in: view) ?? .zero
        return abs(velocity.x) > abs(velocity.y)
    }

    @objc private func touched(_ recognizer: UILongPressGestureRecognizer) {
        switch recognizer.state {
        case .began:
            guard spring.isRunning else { return }
            // Freeze the flight where it is. The strip state stays live;
            // what happens next decides whether it resumes or turns into
            // a drag.
            frozen = FrozenSpring(
                role: springRole,
                target: spring.target,
                velocity: spring.currentVelocity
            )
            spring.cancel()
            updateHoldLoop()
        case .ended, .cancelled, .failed:
            resumeFrozenSpring()
        default:
            break
        }
    }

    /// A bare touch-up over a frozen spring: the interrupted turn
    /// finishes toward its original goal — re-deciding from the frozen
    /// position would silently revert a turn the reader already saw
    /// commit.
    private func resumeFrozenSpring() {
        guard let frozen else { return }
        self.frozen = nil
        // A vanished strip (spread torn down mid-freeze) must resolve the
        // interaction, not orphan it: a bare return here would leave
        // `interactionActive` and the hold loop running forever.
        switch frozen.role {
        case .inner:
            guard let inner = surface.innerMetrics() else {
                cancelInteraction()
                return
            }
            startInnerSpring(from: inner.offset, to: frozen.target, velocity: frozen.velocity)
        case .outerCommit, .outerReturn:
            guard let outer = surface.outerMetrics() else {
                cancelInteraction()
                return
            }
            startOuterSpring(
                from: outer.offset,
                to: frozen.target,
                velocity: frozen.velocity,
                role: frozen.role
            )
        }
    }

    @objc private func panned(_ recognizer: UIPanGestureRecognizer) {
        guard let view = recognizer.view else { return }
        switch recognizer.state {
        case .began:
            onPageTurnGesture?()
            frozen = nil
            spring.cancel()
            // A commit still holds `interactionActive`, so it must gate
            // unconditionally: falling through would re-adopt the
            // mid-commit strip — the *previous* resource's baselines —
            // and a second flick would double-commit through them.
            if committing || (surface.isBusy && !interactionActive) {
                // Landing right on a commit or jump: honor the gesture by
                // deferring the baseline to the first quiet frame instead
                // of dropping the flick — the old pipeline's second
                // failure shape.
                awaitingBaseline = true
                lastTranslationX = recognizer.translation(in: view).x
                return
            }
            guard adoptOrCaptureBaselines() else {
                recognizer.state = .cancelled
                return
            }
            // Applying from zero hands the slop distance to the page too,
            // the way a scroll view tracks from touch-down.
            lastTranslationX = 0
            applyTranslation(recognizer.translation(in: view).x)
        case .changed:
            if awaitingBaseline {
                guard !surface.isBusy, !committing else {
                    lastTranslationX = recognizer.translation(in: view).x
                    return
                }
                awaitingBaseline = false
                guard adoptOrCaptureBaselines() else {
                    recognizer.state = .cancelled
                    return
                }
                lastTranslationX = recognizer.translation(in: view).x
            }
            guard interactionActive else { return }
            applyTranslation(recognizer.translation(in: view).x)
        case .ended, .cancelled, .failed:
            // A lift while still awaiting must not release: the live
            // strip state belongs to whatever the gesture was waiting
            // out, not to this gesture.
            let wasAwaiting = awaitingBaseline
            awaitingBaseline = false
            guard !wasAwaiting, interactionActive else { return }
            let fingerVelocity = recognizer.state == .ended
                ? recognizer.velocity(in: view).x
                : 0
            release(contentVelocity: -fingerVelocity, cancelled: recognizer.state != .ended)
        default:
            break
        }
    }

    private func applyTranslation(_ translationX: CGFloat) {
        let deltaX = translationX - lastTranslationX
        lastTranslationX = translationX
        stripRaw -= deltaX
        applyStrip()
    }

    // MARK: Baselines

    /// A fresh capture, or — mid-boundary — re-adoption of the live
    /// interaction a touch interrupted, so the displaced strip sticks to
    /// the finger seamlessly.
    private func adoptOrCaptureBaselines() -> Bool {
        if interactionActive, abs(boundaryDisplacement) > 0.5 {
            guard let outer = surface.outerMetrics() else { return false }
            // Rubber resistance is not inverted here; the raw coordinate
            // restarts from the displayed one and resistance re-applies
            // from there — imperceptible, and always convergent.
            stripRaw = exitBound + (outer.offset - outerHome)
            boundaryDisplacement = outer.offset - outerHome
            return true
        }
        return captureBaselines()
    }

    private func captureBaselines() -> Bool {
        guard let inner = surface.innerMetrics(), let outer = surface.outerMetrics() else {
            return false
        }
        innerRange = inner.range
        pageWidth = inner.pageWidth
        outerHome = outer.offset
        outerRange = outer.range
        stripRaw = inner.offset
        startPage = pageWidth > 0 ? Int((inner.offset / pageWidth).rounded()) : 0
        exitBound = innerRange.upperBound
        boundaryDisplacement = 0
        neighborVerdictRight = 0
        neighborVerdictLeft = 0
        interactionActive = true
        boundaryHaptic.prepare()
        return true
    }

    /// Can a tap that lands during a return settle be honoured by a turn
    /// right away? Either the strip still holds a page in `direction`, or a
    /// loaded neighbor can be revealed. Consulted before the settle is
    /// cancelled, so a refused turn never costs the settle its animation.
    private func returnAbortCanTurn(direction: CGFloat) -> Bool {
        if let inner = surface.innerMetrics() {
            let target = inner.offset + direction * inner.pageWidth
            if target >= inner.range.lowerBound - 2, target <= inner.range.upperBound + 2 {
                return true
            }
        }
        return neighborExists(direction: direction)
    }

    /// Per-gesture neighbor verdicts: 0 unknown, 1 ready, -1 declined.
    /// The readiness walk runs once per side per interaction.
    private var neighborVerdictRight = 0
    private var neighborVerdictLeft = 0

    /// A neighbor the strip can reveal: one exists in the outer range
    /// *and* it is loaded enough to show — the renderer keeps in-flight
    /// preloads transparent, and dragging one in would slide a blank
    /// sheet across the screen.
    private func neighborExists(direction: CGFloat) -> Bool {
        let inRange = direction > 0
            ? outerHome + pageWidth <= outerRange.upperBound + 0.5
            : outerHome - pageWidth >= outerRange.lowerBound - 0.5
        guard inRange else { return false }
        let toRight = direction > 0
        let cached = toRight ? neighborVerdictRight : neighborVerdictLeft
        if cached != 0 { return cached > 0 }
        let ready = surface.neighborIsReady(toRight: toRight)
        if toRight {
            neighborVerdictRight = ready ? 1 : -1
        } else {
            neighborVerdictLeft = ready ? 1 : -1
        }
        return ready
    }

    // MARK: The strip

    /// Maps the raw strip coordinate onto the two scroll strips: inner
    /// pages first, overflow to the outer resource strip — clamped to
    /// one resource per gesture, rubber-banded where there is no
    /// neighbor to reveal or past the neighbor's slot.
    private func applyStrip() {
        let innerX = min(max(stripRaw, innerRange.lowerBound), innerRange.upperBound)
        let overflow = stripRaw - innerX

        var displayed: CGFloat = 0
        if overflow != 0 {
            exitBound = overflow > 0 ? innerRange.upperBound : innerRange.lowerBound
            if neighborExists(direction: overflow) {
                let clamped = min(max(overflow, -pageWidth), pageWidth)
                displayed = clamped + ReaderPagerRules.rubberBand(overflow - clamped, limit: pageWidth * 0.5)
            } else {
                displayed = ReaderPagerRules.rubberBand(overflow, limit: pageWidth * 0.5)
            }
        }

        // Inner writes only when the value moves: mid-boundary the
        // "visible" spread flips to the neighbor once it covers the
        // center, and an unconditional write would scroll the wrong
        // resource.
        if abs(innerX - lastInnerWritten) > 0.01 || boundaryDisplacement == 0 {
            surface.setInnerOffset(innerX)
            lastInnerWritten = innerX
        }
        boundaryDisplacement = displayed
        surface.setOuterOffset(outerHome + displayed)
        updateHoldLoop()
    }

    private var lastInnerWritten: CGFloat = .nan

    // MARK: Release

    private func release(contentVelocity: CGFloat, cancelled: Bool) {
        let velocity = cancelled ? 0 : contentVelocity
        if abs(boundaryDisplacement) > 0.5 {
            let commits = !cancelled &&
                neighborExists(direction: boundaryDisplacement) &&
                ReaderPagerRules.boundaryCommits(
                    displacement: boundaryDisplacement,
                    velocity: velocity,
                    pageWidth: pageWidth
                )
            let forward = boundaryDisplacement > 0
            startOuterSpring(
                from: outerHome + boundaryDisplacement,
                to: commits ? outerHome + (forward ? pageWidth : -pageWidth) : outerHome,
                velocity: velocity,
                role: commits ? .outerCommit(toRight: forward) : .outerReturn
            )
        } else {
            let innerX = min(max(stripRaw, innerRange.lowerBound), innerRange.upperBound)
            let maxPage = pageWidth > 0
                ? Int((innerRange.upperBound / pageWidth).rounded())
                : 0
            let targetPage = ReaderPagerRules.innerTargetPage(
                startPage: startPage,
                offset: innerX,
                velocity: velocity,
                pageWidth: pageWidth,
                maxPage: maxPage
            )
            let unclamped = CGFloat(targetPage) * pageWidth
            startInnerSpring(
                from: innerX,
                // `maxPage` rounds, so the last page of a resource whose
                // content overruns its column grid would otherwise settle
                // past the scrollable maximum.
                to: min(max(unclamped, innerRange.lowerBound), innerRange.upperBound),
                velocity: velocity
            )
        }
    }

    // MARK: Springs

    private func startInnerSpring(from: CGFloat, to target: CGFloat, velocity: CGFloat) {
        springRole = .inner
        updateHoldLoop()
        guard !UIAccessibility.isReduceMotionEnabled else {
            surface.setInnerOffset(target)
            lastInnerWritten = target
            interactionActive = false
            return
        }
        spring.start(from: from, velocity: velocity, target: target) { [weak self] position, _ in
            guard let self else { return false }
            self.surface.setInnerOffset(position)
            self.lastInnerWritten = position
            return true
        } onSettle: { [weak self] in
            self?.interactionActive = false
        }
    }

    private func startOuterSpring(from: CGFloat, to target: CGFloat, velocity: CGFloat, role: SpringRole) {
        springRole = role
        updateHoldLoop()
        guard !UIAccessibility.isReduceMotionEnabled else {
            surface.setOuterOffset(target)
            boundaryDisplacement = target - outerHome
            outerSpringSettled()
            return
        }
        spring.start(from: from, velocity: velocity, target: target) { [weak self] position, _ in
            guard let self else { return false }
            self.surface.setOuterOffset(position)
            self.boundaryDisplacement = position - self.outerHome
            return true
        } onSettle: { [weak self] in
            self?.outerSpringSettled()
        }
    }

    private func outerSpringSettled() {
        switch springRole {
        case .inner:
            break
        case .outerReturn:
            interactionActive = false
            boundaryDisplacement = 0
            updateHoldLoop()
        case let .outerCommit(toRight):
            committing = true
            boundaryHaptic.impactOccurred(intensity: 0.7)
            commitTask?.cancel()
            commitTask = Task { [weak self] in
                guard let self else { return }
                let moved = await self.surface.commitBoundaryCrossing(toRight: toRight)
                // Invalidated by `cancelInteraction()`: the state below was
                // already reset there, and the screen belongs to whatever
                // re-layout triggered the cancel.
                guard !Task.isCancelled else { return }
                // The renderer's bookkeeping is done the moment the move
                // resolves, so the gate lifts here — the very next tap or
                // flick after a chapter crossing is honored, and only the
                // verification rides on behind it.
                self.committing = false
                self.interactionActive = false
                self.boundaryDisplacement = 0
                self.updateHoldLoop()
                guard moved else {
                    // The renderer refused (a raced jump owns the reader
                    // now): put the strip back on its committed origin
                    // and let the renderer's own move land.
                    self.surface.setOuterOffset(self.outerHome)
                    return
                }
                // The commit shifted the preload window; new spreads
                // arrive with native gestures enabled.
                self.surface.suppressNativeGestures()
                let landed = await self.surface.verifyBoundaryCommit()
                guard !Task.isCancelled, !landed else { return }
                // The rare swallowed commit. The renderer's own layout
                // already snaps its strip back to the unmoved index; the
                // explicit write only makes that deterministic — and only
                // while nothing newer owns the screen.
                if !self.interactionActive, !self.spring.isRunning, self.frozen == nil, !self.committing {
                    self.surface.setOuterOffset(self.outerHome)
                }
            }
        }
    }

    // MARK: Hold loop

    /// Runs whenever a boundary displacement is on screen with no spring
    /// driving it (a held drag, a frozen flight) — the per-frame re-write
    /// that makes the renderer's layout resets self-healing.
    private func updateHoldLoop() {
        let shouldHold = interactionActive &&
            abs(boundaryDisplacement) > 0.5 &&
            !spring.isRunning &&
            !committing
        if shouldHold, !holdLoop.isRunning {
            holdLoop.start { [weak self] _ in
                guard let self,
                      self.interactionActive,
                      abs(self.boundaryDisplacement) > 0.5,
                      !self.spring.isRunning,
                      !self.committing
                else { return false }
                self.surface.setOuterOffset(self.outerHome + self.boundaryDisplacement)
                return true
            }
        } else if !shouldHold {
            holdLoop.stop()
        }
    }
}
