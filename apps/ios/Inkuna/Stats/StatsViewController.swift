import UIKit

/// The Stats tab: reading facts, the month's reading calendar, and the
/// in-progress list — every number from the core's recorded reading
/// sessions, bucketed in the device's local time.
final class StatsViewController: ScrollScreenViewController {
    /// Everything below the title, rebuilt whenever fresh numbers arrive.
    /// Until the first answer the screen shows the title alone — never
    /// zeros pretending to be data.
    private let statsStack = UIStackView()

    /// The in-flight fetch; a new reload cancels its predecessor.
    private var reloadTask: Task<Void, Never>?
    /// `nonisolated(unsafe)` so the nonisolated `deinit` can unregister it.
    /// Only ever touched on the main actor: assigned in `viewDidLoad`, read
    /// once at deinit, when no other reference to this screen survives.
    nonisolated(unsafe) private var libraryDidChangeObserver: NSObjectProtocol?

    deinit {
        if let libraryDidChangeObserver {
            NotificationCenter.default.removeObserver(libraryDidChangeObserver)
        }
    }

    override func viewDidLoad() {
        super.viewDidLoad()

        // TODO(l10n): localize once the strings pass lands.
        let title = displayTitle("Reading")
        contentStack.addArrangedSubview(title)
        contentStack.setCustomSpacing(InkSpacing.space6, after: title)

        statsStack.axis = .vertical
        contentStack.addArrangedSubview(statsStack)

        libraryDidChangeObserver = NotificationCenter.default.addObserver(
            forName: .inkunaLibraryDidChange,
            object: nil,
            queue: .main
        ) { [weak self] _ in
            MainActor.assumeIsolated { self?.reload() }
        }

        reload()
    }

    override func viewWillAppear(_ animated: Bool) {
        super.viewWillAppear(animated)
        // A reading session may have ended since the numbers were fetched.
        reload()
    }

    private func reload() {
        reloadTask?.cancel()
        // Stats bucketing follows the wall clock and week convention this
        // device lives in; the core stores UTC instants and buckets them
        // against exactly this offset.
        let tzOffsetMinutes = Int32(TimeZone.current.secondsFromGMT() / 60)
        let weekStartsMonday = Calendar.current.firstWeekday == 2
        reloadTask = Task { [weak self] in
            do {
                let bookshelf = try await LibraryStore.shared.library()
                let overview = try await bookshelf.statsOverview(
                    tzOffsetMinutes: tzOffsetMinutes,
                    weekStartsMonday: weekStartsMonday
                )
                // Reading, not unfinished: "in progress" means actually
                // begun, so a freshly imported book stays off this list.
                let inProgress = try await bookshelf.list(shelf: .reading, sort: .recentlyOpened)
                guard !Task.isCancelled, let self else { return }
                self.render(overview: overview, inProgress: inProgress)
            } catch {
                // Keep whatever numbers are already on screen; the library
                // screen owns the recovery path for a library that will
                // not open.
            }
        }
    }

    private func render(overview: StatsOverview, inProgress: [Publication]) {
        statsStack.arrangedSubviews.forEach { $0.removeFromSuperview() }

        // TODO(l10n): localize once the strings pass lands.
        let facts = UIStackView(arrangedSubviews: [
            factCard(value: "\(overview.pagesThisWeek)", caption: "pages this week"),
            factCard(value: Self.hoursText(minutes: Int(overview.minutesThisMonth)), caption: "hours this month"),
            factCard(value: "\(overview.booksFinishedThisYear)", caption: "books this year"),
        ])
        facts.axis = .horizontal
        facts.spacing = InkSpacing.space3
        facts.distribution = .fillEqually
        statsStack.addArrangedSubview(facts)
        statsStack.setCustomSpacing(34, after: facts)

        let month = MonthGrid()
        let monthTitle = sectionTitle(month.title)
        statsStack.addArrangedSubview(monthTitle)
        statsStack.setCustomSpacing(14, after: monthTitle)

        let readDays = Set(overview.readDays.map(Int.init))
        let calendar = calendarCard(month: month, readDays: readDays)
        statsStack.addArrangedSubview(calendar)
        statsStack.setCustomSpacing(34, after: calendar)

        // TODO(l10n): localize once the strings pass lands.
        let progressTitle = sectionTitle("In progress")
        statsStack.addArrangedSubview(progressTitle)
        statsStack.setCustomSpacing(6, after: progressTitle)

        if inProgress.isEmpty {
            // TODO(l10n): localize once the strings pass lands.
            statsStack.addArrangedSubview(paddedEmptyState("Nothing in progress yet."))
        } else {
            for publication in inProgress {
                statsStack.addArrangedSubview(progressRow(publication))
            }
        }
    }

    /// Whole and half hours, the design's "6½" voice: 30+ leftover minutes
    /// round to the half, never to a decimal.
    private static func hoursText(minutes: Int) -> String {
        let hours = minutes / 60
        let half = minutes % 60 >= 30
        if hours == 0 {
            return half ? "½" : "0"
        }
        return half ? "\(hours)½" : "\(hours)"
    }

    // MARK: Fact cards

    private func factCard(value: String, caption: String) -> UIView {
        let card = UIView()
        card.backgroundColor = InkColor.bgSurface
        card.layer.cornerRadius = InkRadius.md
        card.installInkShadow(.sm)

        let valueLabel = InkLabel()
        valueLabel.text = value
        valueLabel.font = InkFont.displaySmall
        valueLabel.textColor = InkColor.textDisplay

        let captionLabel = InkLabel()
        captionLabel.text = caption
        captionLabel.font = InkFont.caption
        captionLabel.textColor = InkColor.textSecondary
        captionLabel.numberOfLines = 2

        let stack = UIStackView(arrangedSubviews: [valueLabel, captionLabel])
        stack.axis = .vertical
        stack.spacing = 6
        stack.translatesAutoresizingMaskIntoConstraints = false
        card.addSubview(stack)
        NSLayoutConstraint.activate([
            stack.leadingAnchor.constraint(equalTo: card.leadingAnchor, constant: 14),
            stack.trailingAnchor.constraint(equalTo: card.trailingAnchor, constant: -14),
            stack.topAnchor.constraint(equalTo: card.topAnchor, constant: InkSpacing.space4),
            stack.bottomAnchor.constraint(equalTo: card.bottomAnchor, constant: -InkSpacing.space4),
        ])
        return card
    }

    // MARK: Calendar

    /// The current local month's shape, derived once per render so every
    /// cell agrees on what "today" is.
    private struct MonthGrid {
        let title: String
        /// Weekday header labels, in this locale's column order.
        let weekdaySymbols: [String]
        /// Empty cells before the 1st.
        let leadingBlanks: Int
        let dayCount: Int
        let today: Int

        init(now: Date = Date(), calendar: Calendar = .current) {
            let formatter = DateFormatter()
            formatter.calendar = calendar
            formatter.setLocalizedDateFormatFromTemplate("MMMM")
            title = formatter.string(from: now)

            // Header columns start on this locale's first weekday.
            let symbols = calendar.veryShortStandaloneWeekdaySymbols
            let first = calendar.firstWeekday - 1
            weekdaySymbols = (0..<symbols.count).map { symbols[(first + $0) % symbols.count] }

            let firstOfMonth = calendar.date(
                from: calendar.dateComponents([.year, .month], from: now)
            ) ?? now
            let weekdayOfFirst = calendar.component(.weekday, from: firstOfMonth)
            leadingBlanks = (weekdayOfFirst - calendar.firstWeekday + 7) % 7
            dayCount = calendar.range(of: .day, in: .month, for: now)?.count ?? 30
            today = calendar.component(.day, from: now)
        }
    }

    private func calendarCard(month: MonthGrid, readDays: Set<Int>) -> UIView {
        let card = UIView()
        card.backgroundColor = InkColor.bgSurface
        card.layer.cornerRadius = InkRadius.lg
        card.installInkShadow(.sm)

        let grid = UIStackView()
        grid.axis = .vertical
        grid.spacing = InkSpacing.space1

        let headers = UIStackView(
            arrangedSubviews: month.weekdaySymbols.map { day in
                let label = InkLabel()
                label.text = day
                label.font = InkFont.caption
                label.textColor = InkColor.textTertiary
                label.textAlignment = .center
                return label
            }
        )
        headers.axis = .horizontal
        headers.distribution = .fillEqually
        headers.spacing = InkSpacing.space1
        grid.addArrangedSubview(headers)

        var cells: [UIView] = (0..<month.leadingBlanks).map { _ in UIView() }
        cells += (1...month.dayCount).map { day in
            dayCell(day, today: month.today, didRead: readDays.contains(day))
        }
        while cells.count % 7 != 0 {
            cells.append(UIView())
        }
        for weekStart in stride(from: 0, to: cells.count, by: 7) {
            let week = UIStackView(arrangedSubviews: Array(cells[weekStart..<weekStart + 7]))
            week.axis = .horizontal
            week.distribution = .fillEqually
            week.spacing = InkSpacing.space1
            grid.addArrangedSubview(week)
        }

        let footer = InkLabel()
        // TODO(l10n): localize once the strings pass lands.
        footer.text = readDays.count == 1
            ? "One evening with a book this month."
            : "\(readDays.count) evenings with a book this month."
        footer.font = InkFont.caption
        footer.textColor = InkColor.textTertiary

        let stack = UIStackView(arrangedSubviews: [grid, footer])
        stack.axis = .vertical
        stack.spacing = 10
        stack.translatesAutoresizingMaskIntoConstraints = false
        card.addSubview(stack)
        NSLayoutConstraint.activate([
            stack.leadingAnchor.constraint(equalTo: card.leadingAnchor, constant: 14),
            stack.trailingAnchor.constraint(equalTo: card.trailingAnchor, constant: -14),
            stack.topAnchor.constraint(equalTo: card.topAnchor, constant: InkSpacing.space4),
            stack.bottomAnchor.constraint(equalTo: card.bottomAnchor, constant: -InkSpacing.space4),
        ])
        return card
    }

    private func dayCell(_ day: Int, today: Int, didRead: Bool) -> UIView {
        let isToday = day == today
        let isFuture = day > today

        let cell = UIView()
        cell.backgroundColor = isToday ? InkColor.accentSoft : .clear
        cell.layer.cornerRadius = InkRadius.sm
        cell.heightAnchor.constraint(greaterThanOrEqualToConstant: 36).isActive = true

        let number = InkLabel()
        number.text = "\(day)"
        number.font = InkFont.sans(13, weight: .regular, style: .footnote)
        number.textColor = isFuture ? InkColor.textTertiary : InkColor.textDisplay

        let dot = UIView()
        dot.backgroundColor = didRead ? InkColor.accent : .clear
        dot.layer.cornerRadius = 2
        NSLayoutConstraint.activate([
            dot.widthAnchor.constraint(equalToConstant: 4),
            dot.heightAnchor.constraint(equalToConstant: 4),
        ])

        let stack = UIStackView(arrangedSubviews: [number, dot])
        stack.axis = .vertical
        stack.alignment = .center
        stack.spacing = 3
        stack.translatesAutoresizingMaskIntoConstraints = false
        cell.addSubview(stack)
        NSLayoutConstraint.activate([
            stack.centerXAnchor.constraint(equalTo: cell.centerXAnchor),
            stack.centerYAnchor.constraint(equalTo: cell.centerYAnchor),
        ])
        return cell
    }

    // MARK: In-progress rows

    private func progressRow(_ publication: Publication) -> UIView {
        let title = InkLabel()
        title.text = publication.title
        title.font = InkFont.serif(15, weight: .medium, style: .subheadline)
        title.textColor = InkColor.textDisplay
        title.lineBreakMode = .byTruncatingTail

        let bar = InkProgressBar(progress: CGFloat(publication.progression))
        bar.widthAnchor.constraint(equalToConstant: 92).isActive = true

        let percent = InkLabel()
        percent.text = "\(Int((publication.progression * 100).rounded()))%"
        percent.font = InkFont.caption
        percent.textColor = InkColor.textTertiary
        percent.textAlignment = .right
        percent.widthAnchor.constraint(greaterThanOrEqualToConstant: 34).isActive = true
        percent.setContentHuggingPriority(.required, for: .horizontal)
        percent.setContentCompressionResistancePriority(.required, for: .horizontal)

        let row = UIStackView(arrangedSubviews: [title, bar, percent])
        row.axis = .horizontal
        row.alignment = .center
        row.spacing = 14
        row.isLayoutMarginsRelativeArrangement = true
        row.layoutMargins = UIEdgeInsets(top: 13, left: 0, bottom: 13, right: 0)

        let container = UIView()
        row.translatesAutoresizingMaskIntoConstraints = false
        container.addSubview(row)

        let separator = UIView()
        separator.backgroundColor = InkColor.borderHairline
        separator.translatesAutoresizingMaskIntoConstraints = false
        container.addSubview(separator)

        NSLayoutConstraint.activate([
            row.leadingAnchor.constraint(equalTo: container.leadingAnchor),
            row.trailingAnchor.constraint(equalTo: container.trailingAnchor),
            row.topAnchor.constraint(equalTo: container.topAnchor),
            row.bottomAnchor.constraint(equalTo: container.bottomAnchor),
            separator.leadingAnchor.constraint(equalTo: container.leadingAnchor),
            separator.trailingAnchor.constraint(equalTo: container.trailingAnchor),
            separator.bottomAnchor.constraint(equalTo: container.bottomAnchor),
            separator.heightAnchor.constraint(equalToConstant: 1 / traitCollection.displayScale),
        ])
        return container
    }
}
