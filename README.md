<div align="center">
  
# QuickStatic 🚀🦀🔥

The first and fast static site generator built in Rust specifically for [Djot](https://djot.net) (a more powerful but strict markdown dialect).

</div>

---

## Table of Contents

- [Priorities](#priorities)
- [Installation](#installation)
- [Basic Usage](#basic-usage)
- [Themes and Templates](#themes-and-templates)
- [Contributors Guide](#contributors-guide)
- [License](#license)

---

## Priorities 

A QuickStatic site should show the content first, and hide themes or configuration details. We spend more time writing content than editing website templates and configs. We should lean into features in markdown, such as directives and custom attributes, for adding features and components to markdown, instead of supporting MDX. QuickStatic generates static websites and focuses on doing that in a flexible and fast way. This project prioritizes:

- Simplicity.
- Stability (few features—and should not change very much).
- Content first. 
- Super flexible markdown, via extended markdown specifications.
- Never support MDX (React in markdown). Instead, depend on markdown directives and the markdown attributes extended spec.
- No asset management or SCSS or LESS or any other automated transpiler support. 
- Build on top of Shopify liquid templates. 
- Prioritize productivity.

## Installation

QuickStatic can only be installed using the command below. If you don't have Rust Caro installed already, you can learn how to do so using the [Cargo Installation Guide](https://doc.rust-lang.org/cargo/getting-started/installation.html).

```
cargo install quickstatic
```

## Basic Usage

- Quickstatic themes should be under `_quickstatic/themes/` directory.
- Quickstatic public or build directory is `quickstatic/public`.
- Everything in the root directory gets copied into the output directory in that order, while markdown files are compiled into HTML files.
- Anything with a `.liquid` extension is executed as a template and the `.liquid` extension is striped. E.g., `sitemap.xml.liquid` would be evaluated and become `sitemap.xml`.

To build the site, kindly run the command below:

```
quickstatic build
```

To watch for changes and continuously rebuild the site, kindly run the command below:

```
quickstatic serve
```

## Themes and Templates 

QuickStatic themes can be written using the Shopify liquid templating language. You can learn more about it [here](https://github.com/Shopify/liquid/wiki/Liquid-for-Designers).

To use any given template file for a particular page, simply reference the template file from the frontmatter. 

For example:

```markdown
---
title: Page title
layout: themeName/blog/index.liquid
---

Page Content.

```

This is flexible and allows you to reference multiple themes from one QuickStatic site.

All themes exist in the `_quickstatic/themes/` folder. Your QuickStatic site comes with a default theme named `default`.

## Contributors Guide

1. Fork the repository ([learn how to do this](https://help.github.com/articles/fork-a-repo)).

2. Clone the forked repository like so:

```bash
git clone https://github.com/<your username>/<repository name>.git && cd repository name
```

3. Install the required dependencies and configurations.

4. Create a new branch like so:

```bash
git checkout -b <new-branch-name>
```

5. Make your change(s), add tests, and ensure the tests still pass.

6. Push your commit(s) to your fork and create a pull request ([learn how to do this](https://docs.github.com/en/github/collaborating-with-issues-and-pull-requests/creating-a-pull-request)).

7. Well done! Someone will attend to your pull request, provide some feedback, or merge it.

## License

This repository is published under the [MIT](LICENSE) license.
