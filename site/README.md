# Pad Companion Support site

The page Pad Companion's **Report a bug** button opens: `https://eiza-1.github.io/stadia-pad-companion/`.
Bug reports and feature requests become GitHub issues through the forms in `.github/ISSUE_TEMPLATE/`
(the site pre-fills them; field ids in `site.js` must match the template ids).

## Publish (one time)
1. Create the public repository `Eiza-1/stadia-pad-companion` on GitHub and push this project to its `main` branch.
2. Repository **Settings > Pages > Build and deployment > Source: GitHub Actions**.
3. The `Support site` workflow (`.github/workflows/pages.yml`) publishes `site/` on every push that changes it
   (or run it from the Actions tab).

The labels the templates use (`bug`, `enhancement`) exist in every new GitHub repository.
If the site moves, change `SUPPORT_SITE` in `app/ui/js/device.js` (and `REPO` in `site/site.js` for non-GitHub hosts).

The download page (`download/`) asks the GitHub API for the newest release and links its `*setup.exe` asset
directly, so publishing a new release updates the button with no site change.
