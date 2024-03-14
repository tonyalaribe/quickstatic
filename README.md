# QuickStatic 🚀🦀🔥

## The First static site generator build specifically for [Djot](https://djot.net) (A more powerful but strict Markdown dialect)

A fast static site generator written in Rust 🦀🚀.
A QuickStatic site should show the content first, and hide themes or configuration away. 
We spend more time writing content than editing website templates and configs.

We should lean into features in markdown, such as directives and custom attributes, for adding features and components to markdow, instead of supporting MDX.

## Installation

- Install using Rust cargo for now. Install cargo via: [Cargo installation Guide](https://doc.rust-lang.org/cargo/getting-started/installation.html)
```
cargo install quickstatic
```

## Priorities 

- Simplicity 
- Stability (Few features, and should not change very much)
- Content first. 
- Super flexible markdown, via extended markdown specifications.
- Never support MDX (React in markdown). Instead, depend on markdown directives and the markdown attributes extended spec.
- No asset management or SCSS or LESS or any other automated transpiler support. 
- Build on top of shopify liquid templates. 
- Prioritize productivity

QuickStatic generates static websites, and focuses on doing that in a flexible and fast way.


## Themes and Templates 

QuickStatic themes can be written using the shopify liquid templating language. 
You can learn more about it here ![https://github.com/Shopify/liquid/wiki/Liquid-for-Designers](https://github.com/Shopify/liquid/wiki/Liquid-for-Designers)

To use any given template file for a particular page, simply reference the template file from the frontmatter. 
Eg:

```markdown
---
title: Page title
layout: themeName/blog/index.liquid
---

Page Content.

```

This is flexible, and allows you reference multiple themes from one QuickStatic site.
All themes exist in the `_quickstatic/themes/` folder. Your QuickStatic site comes with a default theme named `default`.


## NOTE 
- Licenced under the MIT License. 
