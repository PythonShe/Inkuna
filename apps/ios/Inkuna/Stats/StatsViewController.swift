import UIKit

/// The Stats tab: reading facts, the month's reading calendar, and the
/// in-progress list.
///
/// TODO(core): every number here comes from the Rust core's progress
/// tracking once sessions are recorded; the placeholder mirrors the design
/// canvas.
final class StatsViewController: ScrollScreenViewController {
    override func viewDidLoad() {
        super.viewDidLoad()

        // TODO(l10n): localize once the strings pass lands.
        let title = displayTitle("Reading")
        contentStack.addArrangedSubview(title)
        contentStack.setCustomSpacing(InkSpacing.space6, after: title)

        let facts = UIStackView(
            arrangedSubviews: PlaceholderLibrary.facts.map { factCard(value: $0.value, caption: $0.caption) }
        )
        facts.axis = .horizontal
        facts.spacing = InkSpacing.space3
        facts.distribution = .fillEqually
        contentStack.addArrangedSubview(facts)
        contentStack.setCustomSpacing(34, after: facts)

        let monthTitle = sectionTitle(PlaceholderLibrary.calendarMonthTitle)
        contentStack.addArrangedSubview(monthTitle)
        contentStack.setCustomSpacing(14, after: monthTitle)

        let calendar = calendarCard()
        contentStack.addArrangedSubview(calendar)
        contentStack.setCustomSpacing(34, after: calendar)

        let progressTitle = sectionTitle("In progress")
        contentStack.addArrangedSubview(progressTitle)
        contentStack.setCustomSpacing(6, after: progressTitle)

        for book in PlaceholderLibrary.books {
            contentStack.addArrangedSubview(progressRow(book))
        }
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

    private func calendarCard() -> UIView {
        let card = UIView()
        card.backgroundColor = InkColor.bgSurface
        card.layer.cornerRadius = InkRadius.lg
        card.installInkShadow(.sm)

        let grid = UIStackView()
        grid.axis = .vertical
        grid.spacing = InkSpacing.space1

        let headers = UIStackView(
            arrangedSubviews: ["S", "M", "T", "W", "T", "F", "S"].map { day in
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

        var cells: [UIView] = (0..<PlaceholderLibrary.calendarLeadingBlanks).map { _ in UIView() }
        cells += (1...PlaceholderLibrary.calendarDayCount).map(dayCell)
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
        footer.text = PlaceholderLibrary.calendarCaption
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

    private func dayCell(_ day: Int) -> UIView {
        let isToday = day == PlaceholderLibrary.calendarToday
        let isFuture = day > PlaceholderLibrary.calendarToday
        let didRead = PlaceholderLibrary.calendarReadDays.contains(day)

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

    private func progressRow(_ book: PlaceholderBook) -> UIView {
        let title = InkLabel()
        title.text = book.title
        title.font = InkFont.serif(15, weight: .medium, style: .subheadline)
        title.textColor = InkColor.textDisplay
        title.lineBreakMode = .byTruncatingTail

        let bar = InkProgressBar(progress: book.progress)
        bar.widthAnchor.constraint(equalToConstant: 92).isActive = true

        let percent = InkLabel()
        percent.text = book.percentText
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
