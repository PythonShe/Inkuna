import UIKit

/// The reader's own pager: every horizontal page turn — within a resource
/// and across resource (chapter) boundaries — is driven here, on our
/// gesture pipeline and our physics, instead of the renderer's. The
/// renderer keeps rendering, preloading, and bookkeeping; a boundary crossing
/// tracks the finger and settles exactly like an inner turn.
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
/// A boundary crossing commits the instant it is decided — at release, or
/// as a flick/tap turn starts — not when its animation lands. The commit
/// relabels the strips onto the neighbor chapter pixel-identically (the
/// landing page becomes the inner strip; the departing page becomes
/// opposite-side boundary travel) and the remaining motion glides home on
/// the raw strip coordinate, so further turns chain onto a crossing as
/// freely as onto an inner turn.
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
    private var lastTranslationX: CGFloat = 0

    // MARK: Animation state

    private let spring = ReaderPagerSpring()
    private enum SpringRole {
        /// Drives the inner offset directly — pure within-chapter settles.
        case inner
        /// Drives the raw strip coordinate, so one flight can carry
        /// boundary travel and inner travel as one continuous motion: the
        /// remaining glide after an eager commit, a cancelled crossing's
        /// return, and any turns chained onto either.
        case glide
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
    /// A crossing asked for while the previous one's departing sheet still
    /// occupies the boundary; it runs when the glide lands.
    private var pendingTurnDirection: CGFloat = 0
    private var pendingTurnVelocity: CGFloat = 0
    /// The boundary displacement on screen is a page already committed
    /// away from — its release runs the full inner rules instead of the
    /// return-to-exit-page rule, so momentum chains into the new chapter.
    private var postCommitGlide = false
    /// Re-asserts the displaced outer offset every frame while a boundary
    /// interaction holds without a running spring.
    private let holdLoop = ReaderPagerFrameLoop()

    private let boundaryHaptic = UIImpactFeedbackGenerator(style: .soft)

    /// Fired the moment turn intent shows — a claimed drag or an edge
    /// tap — so the chrome can clear before the motion, not after it.
    var onPageTurnGesture: (() -> Void)?

    /// Fired when a programmatic turn (edge tap, key, VoiceOver's scroll
    /// action) meets a boundary whose neighbor chapter exists but has not
    /// finished laying out. The host schedules that chapter and completes
    /// the turn on its readiness event; the geometric direction is passed
    /// through.
    var onBoundaryTurnPending: ((CGFloat) -> Void)?

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
        // Always enabled: engageability is a per-gesture question answered
        // in `gestureRecognizerShouldBegin`, never a stored state — a
        // recognizer disabled while the first chapter laid out would need
        // every readiness event to re-enable it, and missing one leaves
        // the reader swipe-dead.
        let pan = UIPanGestureRecognizer(target: self, action: #selector(panned))
        pan.maximumNumberOfTouches = 1
        pan.delegate = self
        view.addGestureRecognizer(pan)
        self.pan = pan
    }

    /// The navigation stack's interactive-pop recognizer, once handed
    /// over: the screen's left edge belongs to it alone.
    private weak var systemBackGesture: UIGestureRecognizer?

    /// Hands the left screen edge to the system back gesture: the pan
    /// waits for it to fail before claiming a drag, so an edge-back swipe
    /// never doubles as a backward page turn. Touches away from the edge
    /// fail it immediately, so page turns keep their responsiveness.
    func yieldToSystemBackGesture(_ recognizer: UIGestureRecognizer?) {
        guard let recognizer, let pan else { return }
        systemBackGesture = recognizer
        pan.require(toFail: recognizer)
    }

    /// Cancels any live interaction and puts the strips back on their
    /// committed alignment — for rotation and teardown, where the
    /// renderer's own re-layout takes over.
    func cancelInteraction() {
        spring.cancel()
        holdLoop.stop()
        frozen = nil
        pendingTurnDirection = 0
        pendingTurnVelocity = 0
        postCommitGlide = false
        if interactionActive {
            surface.setOuterOffset(outerHome)
            interactionActive = false
            boundaryDisplacement = 0
            surface.endPagingInteraction()
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

    private func turn(direction: CGFloat, velocity: CGFloat = 0) -> Bool {
        guard surface.isEngageable, !surface.isBusy else { return false }
        onPageTurnGesture?()

        // A turn already in flight: successive taps chain by moving the
        // running spring's goal one page further. A crossing commits
        // eagerly, so a glide's goal chains exactly like an inner turn's —
        // and one aimed past the strip runs the next crossing the moment
        // the departing sheet is out of the way.
        if spring.isRunning {
            let next = spring.target + direction * pageWidth
            let inInner = next >= innerRange.lowerBound - 2 && next <= innerRange.upperBound + 2
            switch springRole {
            case .inner:
                if inInner {
                    spring.retarget(min(max(next, innerRange.lowerBound), innerRange.upperBound))
                }
                return true
            case .glide:
                if inInner {
                    spring.retarget(min(max(next, innerRange.lowerBound), innerRange.upperBound))
                    return true
                }
                guard neighborExists(direction: direction) else { return true }
                if abs(boundaryDisplacement) <= 0.5 || (boundaryDisplacement > 0) == (direction > 0) {
                    // The boundary is clear, or already displaced toward
                    // that neighbor: cross now, mid-flight.
                    let carried = spring.currentVelocity
                    let fallback = min(max(spring.target, innerRange.lowerBound), innerRange.upperBound)
                    spring.cancel()
                    if !crossBoundary(direction: direction, velocity: carried) {
                        startGlideSpring(to: fallback, velocity: carried)
                    }
                } else {
                    pendingTurnDirection = direction
                    pendingTurnVelocity = velocity
                }
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
                velocity: velocity
            )
            return true
        }
        // A boundary turn: commit eagerly and glide the neighbor in with
        // the same spring a drag-release uses.
        guard neighborExists(direction: direction),
              crossBoundary(direction: direction, velocity: velocity) else {
            interactionActive = false
            surface.endPagingInteraction()
            // The neighbor chapter is there but still laying out: hand
            // the turn to the host, which parks it and completes it on
            // that chapter's readiness event rather than refusing a
            // turn the reader asked for.
            if neighborInOuterRange(direction: direction), let onBoundaryTurnPending {
                onBoundaryTurnPending(direction)
                return true
            }
            return false
        }
        return true
    }

    // MARK: Gesture plumbing

    func gestureRecognizer(
        _ gestureRecognizer: UIGestureRecognizer,
        shouldRecognizeSimultaneouslyWith otherGestureRecognizer: UIGestureRecognizer
    ) -> Bool {
        // Everything coexists except the pan and the system back gesture:
        // one finger must never both pop the reader and turn its page.
        // (The touch observer stays permissive — it begins on every
        // touch-down, and an exclusive answer there would block the back
        // gesture entirely.)
        gestureRecognizer !== pan || otherGestureRecognizer !== systemBackGesture
    }

    func gestureRecognizerShouldBegin(_ gestureRecognizer: UIGestureRecognizer) -> Bool {
        guard gestureRecognizer === pan else { return true }
        guard surface.isEngageable else { return false }
        // Horizontal drags belong to the selection handles while text is
        // selected.
        guard !surface.hasActiveSelection else { return false }
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
        case .glide:
            guard surface.innerMetrics() != nil else {
                cancelInteraction()
                return
            }
            startGlideSpring(to: frozen.target, velocity: frozen.velocity)
        }
    }

    @objc private func panned(_ recognizer: UIPanGestureRecognizer) {
        guard let view = recognizer.view else { return }
        switch recognizer.state {
        case .began:
            onPageTurnGesture?()
            frozen = nil
            spring.cancel()
            pendingTurnDirection = 0
            guard adoptOrCaptureBaselines() else {
                recognizer.state = .cancelled
                return
            }
            // Applying from zero hands the slop distance to the page too,
            // the way a scroll view tracks from touch-down.
            lastTranslationX = 0
            applyTranslation(recognizer.translation(in: view).x)
        case .changed:
            guard interactionActive else { return }
            applyTranslation(recognizer.translation(in: view).x)
        case .ended, .cancelled, .failed:
            guard interactionActive else { return }
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
        surface.beginPagingInteraction()
        guard let inner = surface.innerMetrics(), let outer = surface.outerMetrics() else {
            surface.endPagingInteraction()
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
        postCommitGlide = false
        neighborVerdictRight = 0
        neighborVerdictLeft = 0
        interactionActive = true
        boundaryHaptic.prepare()
        return true
    }

    /// Per-gesture neighbor verdicts: 0 unknown, 1 ready, -1 declined.
    /// The readiness walk runs once per side per interaction.
    private var neighborVerdictRight = 0
    private var neighborVerdictLeft = 0

    /// Whether a neighbor chapter exists on this side of the outer range,
    /// regardless of how far its layout has got.
    private func neighborInOuterRange(direction: CGFloat) -> Bool {
        direction > 0 ? outerRange.upperBound > outerHome : outerRange.lowerBound < outerHome
    }

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
        adoptInnerGrowth()
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
        if displayed == 0 { postCommitGlide = false }
        surface.setOuterOffset(outerHome + displayed)
        updateHoldLoop()
    }

    /// A chapter's published page count grows while it lays out. The
    /// surface only ever reports that growth when no existing offset moves
    /// (LTR appends past the strip's end; RTL stays frozen until the
    /// interaction ends), so adopting the taller range here lets the drag
    /// reach pages published under the finger instead of spilling into a
    /// chapter crossing. The lower bound and the pitch stay as captured.
    private func adoptInnerGrowth() {
        guard let fresh = surface.innerMetrics(),
              fresh.pageWidth == pageWidth,
              fresh.range.upperBound > innerRange.upperBound else { return }
        innerRange = innerRange.lowerBound ... fresh.range.upperBound
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
            if commits, crossBoundary(direction: boundaryDisplacement > 0 ? 1 : -1, velocity: velocity) {
                return
            }
            // Not crossing: glide the strip home. Behind an eager commit
            // the displaced sheet is one already left behind, so the full
            // inner rules run and a same-direction flick chains straight
            // into the new chapter; an uncommitted crossing returns to
            // its exit page.
            startGlideSpring(
                to: postCommitGlide ? innerReleaseTarget(velocity: velocity) : exitBound,
                velocity: velocity
            )
        } else {
            // A fast flick barely travels before it releases: on a
            // resource's edge page the strip never overflows during the
            // touch, so the clamped inner settle below would swallow a
            // turn the reader clearly asked for. Route a flick past the
            // edge into the same eager crossing a dragged commit takes.
            if pageWidth > 0, abs(velocity) >= ReaderPagerRules.flingVelocity {
                let innerX = min(max(stripRaw, innerRange.lowerBound), innerRange.upperBound)
                let maxPage = Int((innerRange.upperBound / pageWidth).rounded())
                let direction: CGFloat = velocity > 0 ? 1 : -1
                let page = innerX / pageWidth
                let flickTarget = velocity > 0
                    ? Int(page.rounded(.down)) + 1
                    : Int(page.rounded(.up)) - 1
                if (direction > 0 && flickTarget > maxPage) || (direction < 0 && flickTarget < 0),
                   neighborExists(direction: direction),
                   crossBoundary(direction: direction, velocity: velocity) {
                    return
                }
            }
            let innerX = min(max(stripRaw, innerRange.lowerBound), innerRange.upperBound)
            startInnerSpring(
                from: innerX,
                to: innerReleaseTarget(velocity: velocity),
                velocity: velocity
            )
        }
    }

    /// The inner offset a release settles on, by the shared rules; a
    /// post-commit flick aimed past the strip's far edge queues the next
    /// crossing for the glide's settle, since the departing sheet still
    /// occupies the boundary.
    private func innerReleaseTarget(velocity: CGFloat) -> CGFloat {
        let innerX = min(max(stripRaw, innerRange.lowerBound), innerRange.upperBound)
        guard pageWidth > 0 else { return innerX }
        // `maxPage` rounds, so the last page of a resource whose content
        // overruns its column grid would otherwise settle past the
        // scrollable maximum.
        let maxPage = Int((innerRange.upperBound / pageWidth).rounded())
        let targetPage = ReaderPagerRules.innerTargetPage(
            startPage: startPage,
            offset: innerX,
            velocity: velocity,
            pageWidth: pageWidth,
            maxPage: maxPage
        )
        if abs(velocity) >= ReaderPagerRules.flingVelocity {
            let direction: CGFloat = velocity > 0 ? 1 : -1
            let page = innerX / pageWidth
            let flickTarget = velocity > 0
                ? Int(page.rounded(.down)) + 1
                : Int(page.rounded(.up)) - 1
            if (direction > 0 && flickTarget > maxPage) || (direction < 0 && flickTarget < 0),
               abs(boundaryDisplacement) > 0.5,
               neighborExists(direction: direction) {
                pendingTurnDirection = direction
                pendingTurnVelocity = velocity
            }
        }
        let unclamped = CGFloat(targetPage) * pageWidth
        return min(max(unclamped, innerRange.lowerBound), innerRange.upperBound)
    }

    // MARK: Springs

    private func startInnerSpring(from: CGFloat, to target: CGFloat, velocity: CGFloat) {
        springRole = .inner
        updateHoldLoop()
        guard !UIAccessibility.isReduceMotionEnabled else {
            surface.setInnerOffset(target)
            lastInnerWritten = target
            interactionActive = false
            surface.endPagingInteraction()
            return
        }
        spring.start(from: from, velocity: velocity, target: target) { [weak self] position, _ in
            guard let self else { return false }
            self.surface.setInnerOffset(position)
            self.lastInnerWritten = position
            return true
        } onSettle: { [weak self] in
            guard let self else { return }
            self.interactionActive = false
            self.surface.endPagingInteraction()
        }
    }

    /// Commits the crossing this instant and rebases the live interaction
    /// onto the neighbor chapter: the landing page becomes the inner strip
    /// and the departing page becomes opposite-side boundary travel,
    /// pixel-identical across the relabel. The remaining travel glides
    /// home on the strip spring, so further turns chain onto it freely.
    /// Returns false when the surface refuses the commit; the strip is
    /// untouched then.
    private func crossBoundary(direction: CGFloat, velocity: CGFloat) -> Bool {
        let travelled = min(abs(boundaryDisplacement), pageWidth)
        guard surface.commitBoundaryCrossing(toRight: direction > 0) else { return false }
        boundaryHaptic.impactOccurred(intensity: 0.7)
        guard let inner = surface.innerMetrics(), let outer = surface.outerMetrics(),
              inner.pageWidth > 0 else {
            // No strip to glide on after the relabel (a generation flipped
            // under the commit): resolve the interaction on the landing.
            cancelInteraction()
            return true
        }
        innerRange = inner.range
        pageWidth = inner.pageWidth
        outerHome = outer.offset
        outerRange = outer.range
        let entry = direction > 0 ? innerRange.lowerBound : innerRange.upperBound
        exitBound = entry
        startPage = Int((entry / pageWidth).rounded())
        stripRaw = entry - direction * (pageWidth - travelled)
        // The commit already presented the landing offset.
        lastInnerWritten = entry
        neighborVerdictRight = 0
        neighborVerdictLeft = 0
        applyStrip()
        postCommitGlide = abs(boundaryDisplacement) > 0.5
        startGlideSpring(to: entry, velocity: velocity)
        return true
    }

    private func startGlideSpring(to target: CGFloat, velocity: CGFloat) {
        springRole = .glide
        updateHoldLoop()
        guard !UIAccessibility.isReduceMotionEnabled else {
            applyStripAt(target)
            glideSettled()
            return
        }
        spring.start(from: stripRaw, velocity: velocity, target: target) { [weak self] position, _ in
            guard let self else { return false }
            self.applyStripAt(position)
            return true
        } onSettle: { [weak self] in
            self?.glideSettled()
        }
    }

    private func applyStripAt(_ position: CGFloat) {
        stripRaw = position
        applyStrip()
    }

    private func glideSettled() {
        interactionActive = false
        boundaryDisplacement = 0
        postCommitGlide = false
        updateHoldLoop()
        surface.endPagingInteraction()
        guard pendingTurnDirection != 0 else { return }
        let direction = pendingTurnDirection
        let velocity = pendingTurnVelocity
        pendingTurnDirection = 0
        pendingTurnVelocity = 0
        _ = turn(direction: direction, velocity: velocity)
    }

    // MARK: Hold loop

    /// Runs whenever a boundary displacement is on screen with no spring
    /// driving it (a held drag, a frozen flight) — the per-frame re-write
    /// that makes the renderer's layout resets self-healing.
    private func updateHoldLoop() {
        let shouldHold = interactionActive &&
            abs(boundaryDisplacement) > 0.5 &&
            !spring.isRunning
        if shouldHold, !holdLoop.isRunning {
            holdLoop.start { [weak self] _ in
                guard let self,
                      self.interactionActive,
                      abs(self.boundaryDisplacement) > 0.5,
                      !self.spring.isRunning
                else { return false }
                self.surface.setOuterOffset(self.outerHome + self.boundaryDisplacement)
                return true
            }
        } else if !shouldHold {
            holdLoop.stop()
        }
    }
}
