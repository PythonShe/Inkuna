import UIKit

/// The shelf. Currently a plain list fed by the Rust core; the crafted
/// Apple Books-style presentation replaces this once Readium lands.
final class LibraryViewController: UITableViewController {
    private var publications: [Publication] = []

    override func viewDidLoad() {
        super.viewDidLoad()
        title = "Inkuna"
        navigationController?.navigationBar.prefersLargeTitles = true
        tableView.register(UITableViewCell.self, forCellReuseIdentifier: "publication")
        reload()
    }

    private func reload() {
        Task {
            publications = (try? await LibraryStore.shared.library.list()) ?? []
            tableView.reloadData()
            updateEmptyState()
        }
    }

    private func updateEmptyState() {
        guard publications.isEmpty else {
            tableView.backgroundView = nil
            return
        }
        let label = UILabel()
        label.text = "Where ink meets moonlight.\ncore \(coreVersion())"
        label.numberOfLines = 0
        label.textAlignment = .center
        label.textColor = .secondaryLabel
        label.font = .preferredFont(forTextStyle: .callout)
        tableView.backgroundView = label
    }

    override func tableView(_ tableView: UITableView, numberOfRowsInSection section: Int) -> Int {
        publications.count
    }

    override func tableView(
        _ tableView: UITableView,
        cellForRowAt indexPath: IndexPath
    ) -> UITableViewCell {
        let cell = tableView.dequeueReusableCell(withIdentifier: "publication", for: indexPath)
        let publication = publications[indexPath.row]
        var content = cell.defaultContentConfiguration()
        content.text = publication.title
        content.secondaryText = publication.authors.joined(separator: ", ")
        cell.contentConfiguration = content
        return cell
    }
}
