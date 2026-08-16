// Resolves the newest Android test-build APK at build time (the site is
// static and ships no client JS). deploy-website.yml re-runs on every
// release publish, so the baked-in link tracks the latest android-v* tag.
const RELEASES_PAGE = "https://github.com/PythonShe/Inkuna/releases";

interface ReleaseAsset {
  name: string;
  browser_download_url: string;
}

interface Release {
  tag_name?: string;
  draft?: boolean;
  assets?: ReleaseAsset[];
}

export async function latestAndroidApkUrl(): Promise<string> {
  try {
    const headers: Record<string, string> = {
      Accept: "application/vnd.github+json",
    };
    // CI passes GITHUB_TOKEN to dodge the unauthenticated rate limit.
    if (process.env.GITHUB_TOKEN) {
      headers.Authorization = `Bearer ${process.env.GITHUB_TOKEN}`;
    }
    const res = await fetch(
      "https://api.github.com/repos/PythonShe/Inkuna/releases?per_page=30",
      { headers },
    );
    if (!res.ok) throw new Error(`GitHub API ${res.status}`);
    const releases = (await res.json()) as Release[];
    for (const release of releases) {
      // Newest first; test builds are published as prereleases, only
      // drafts are invisible to the public and skipped.
      if (!release.tag_name?.startsWith("android-v") || release.draft) continue;
      const apk = release.assets?.find((a) => a.name.endsWith(".apk"));
      if (apk) return apk.browser_download_url;
    }
  } catch {
    // Offline local builds and API hiccups fall through to the page link.
  }
  return RELEASES_PAGE;
}
