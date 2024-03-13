use eyre::{Result, WrapErr};
use gray_matter::engine::YAML;
use gray_matter::Matter;
use indexmap::IndexMap;
use liquid::{model::ValueView, ObjectView};
use liquid_core::partials::{EagerCompiler, InMemorySource, OnDemandCompiler};
use djotters;
use serde::{Deserialize, Serialize};
use serde_yaml::Value;
use std::{
    collections::HashMap,
    fs::{self, create_dir_all, File},
    io::{self, Read, Write},
    path::Path,
};
mod base_cli;
mod where_glob;
use base_cli::Commands;
use clap::Parser;

#[derive(Clone, Debug, Serialize)]
struct DocumentData {
    file_path: String,
    html_file_path: String,
    markdown_body: String,
    markdown_processed: String,
    html: String,
    frontmatter: Value,
    permalink: String,
}

// Config struct represents a key value tree of everything in the quickstatic config file.
// The quickstatic config file should be at: <static_site_>
#[derive(Deserialize, Serialize, Debug)]
struct Config {
    base_url: String,
    title: String,
    theme: String,
    layouts: IndexMap<String, String>,
    ignore: Vec<String>,
    #[serde(skip_deserializing)]
    raw: Value,
}

#[derive(Debug, Serialize)]
struct RenderContext<'a> {
    config: &'a Config,
    this: &'a mut DocumentData,
    file_list: &'a Vec<DocumentData>,
}

fn get_file_paths_recursive(
    config_struct: &Config,
    dir: &Path,
    exclude_dir_names: &Vec<&str>,
    extension: &str,
) -> Vec<String> {
    let mut paths = Vec::new();

    if dir.is_dir() {
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if config_struct
                    .ignore
                    .iter()
                    .find(|glb| glob_match::glob_match(&glb, &path.to_str().unwrap()))
                    .is_some()
                {
                    continue;
                }

                if path.is_dir() {
                    if exclude_dir_names
                        .contains(&path.file_name().unwrap_or_default().to_str().unwrap())
                    {
                        continue; // Skip this directory and continue with the next entry
                    }
                    // If it's a directory, recursively get the files within it
                    paths.extend(get_file_paths_recursive(
                        config_struct,
                        &path,
                        exclude_dir_names,
                        extension,
                    ));
                } else {
                    // If it's a file, add its path to the vector
                    if let Some(path_str) = path.to_str() {
                        let ext_with_dot = ".".to_owned() + extension;
                        if path_str.to_string().ends_with(&ext_with_dot) {
                            paths.push(path_str.to_string());
                        }
                    }
                }
            }
        }
    }

    paths
}

fn copy_recursive(
    config_struct: &Config,
    src: &Path,
    exclude_dir_names: &Vec<&str>,
    dest: &Path,
) -> io::Result<()> {
    if src.is_dir() {
        if !dest.exists() {
            fs::create_dir_all(dest).expect(&format!("copy_recursive failed to create {dest:?}"));
        }

        for entry in fs::read_dir(src)? {
            let entry = entry?;
            let path = entry.path();
            let new_dest = dest.join(entry.file_name());

            if config_struct
                .ignore
                .iter()
                .find(|glb| glob_match::glob_match(&glb, &path.to_str().unwrap()))
                .is_some()
            {
                continue;
            }

            if path.is_dir() {
                let file_name = path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();
                if exclude_dir_names.contains(&file_name.as_str()) || file_name.starts_with(".") {
                    continue; // Skip this directory and continue with the next entry
                }
                copy_recursive(config_struct, &path, exclude_dir_names, &new_dest)?;
            } else {
                if !path
                    .file_name()
                    .expect(&format!("copy_recursive: failed to get file name {path:?}"))
                    .to_str()
                    .expect(&format!(
                        "copy_recursive: failed converting os string to string for path {path:?}"
                    ))
                    .ends_with(".md")
                {
                    fs::copy(&path, &new_dest)
                        .expect(&format!("copy_recursive: failed to copy {path:?}"));
                }
            }
        }
    } else {
        if let Some(parent) = dest.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent)?;
            }
        }
        fs::copy(src, dest)?;
    }
    Ok(())
}

fn main() -> eyre::Result<()> {
    let cli_instance = base_cli::Cli::parse();

    match cli_instance.command {
        None => build("./"),
        Some(Commands::Build { dir }) => build(&dir),
    }
}

fn find_template(layouts_map: IndexMap<String, String>, file_path: String) -> eyre::Result<String> {
    for (k, v) in &layouts_map {
        if glob_match::glob_match(&k, &file_path.clone()) {
            return Ok(v.into());
        };
    }

    Err(eyre::eyre!("expecting a general layout glob such as **/*.md to be set in the ./quickstatic.yaml config file: {:?} layout_map: {:?}", file_path, layouts_map))
}

fn build(root_dir: &str) -> eyre::Result<()> {
    let dir = Path::new(root_dir); // Specify the directory

    // Read the config into a config struct.
    let config_file_content = fs::read_to_string(dir.join("quickstatic.yaml"))
        .expect("unable to find `quickstatic.yaml` config file");
    let config_value = serde_yaml::from_str::<Value>(&config_file_content).unwrap();
    let mut config_struct: Config = serde_yaml::from_value(config_value.clone())?;
    config_struct.raw = config_value;

    let exclude_dir_names = vec!["_quickstatic", "public", ".git", "node_modules"];
    copy_recursive(
        &config_struct,
        dir,
        &exclude_dir_names,
        Path::new(&format!("{root_dir}/public/")),
    )?;
    let file_paths = get_file_paths_recursive(&config_struct, dir, &exclude_dir_names, "md");

    // matter will hold the parsed version of frontmatter from the markdown documents.
    // frontmatter is extra metadata associated with markdown content. It is usually at the top of
    // the markdown file, and would be in a format such as:
    // ``` markdown
    // ---
    // key: value
    // key2: value2
    // ---
    //
    // # Markdown content
    // ```
    //
    // key and key2 are keys in the frontmatter associated with the markdown above.
    let matter = Matter::<YAML>::new();
    let themes_dir = format!("{root_dir}/_quickstatic/themes");
    create_dir_all(&themes_dir)?;
    let liquid_source_map = read_partials_from_directory(Path::new(&themes_dir), "liquid")?;

    let mut liquid_mem_source = InMemorySource::new();
    for (fp, src) in liquid_source_map {
        liquid_mem_source.add(fp, src);
    }

    let partials_compiler = EagerCompiler::new(liquid_mem_source);
    // TODO: do this in a new loop, so the context can contain the entire render tree, to
    // support referencing other documents in the template. Eg in table of content pages.
    // or listing categories and tags.
    let builder = liquid::ParserBuilder::with_stdlib()
        .filter(crate::where_glob::WhereGlob)
        .filter(liquid_lib::jekyll::Slugify)
        .filter(liquid_lib::jekyll::Push)
        .filter(liquid_lib::jekyll::Pop)
        .filter(liquid_lib::jekyll::Unshift)
        .filter(liquid_lib::jekyll::Shift)
        .filter(liquid_lib::jekyll::ArrayToSentenceString)
        .filter(liquid_lib::shopify::Pluralize);
    let parser_builder = builder.partials(partials_compiler).build().unwrap();

    // documents_map holds the entire documents tree and can be iterated eg via prefix.
    let mut documents_list = vec![];

    for file_path in file_paths.clone() {
        let file_path_str = file_path.to_string().to_owned();
        let file_path_no_root = file_path_str.strip_prefix(root_dir).unwrap();

        let contents = fs::read_to_string(file_path.clone())?;
        let result = matter.parse(&contents);
        let frontmatter: Value = result
            .data
            .unwrap_or(gray_matter::Pod::Null)
            .clone()
            .deserialize()?;

        let html_file_path = file_path_no_root.strip_suffix(".md").unwrap().to_owned() + ".html";
        let public_root_path = Path::new(root_dir)
            .join("public")
            .to_string_lossy()
            .to_owned()
            .to_string();
        let final_html_file_path = format!("{}{}", public_root_path, html_file_path);

        let document = DocumentData {
            file_path: file_path.clone(),
            html_file_path: final_html_file_path.clone(),
            markdown_body: result.content.clone(),
            markdown_processed: "".into(),
            frontmatter: frontmatter.clone(),
            html: "".into(),
            permalink: final_html_file_path.clone().to_string()
                .trim_end_matches("index.html")
                .trim_left_matches("./public")
                .to_owned(),
        };

        documents_list.push(document);
    }

    let documents_list_clone = documents_list.clone();
    for mut document in documents_list {
        let mut render_ctx = &mut RenderContext {
            config: &config_struct,
            this: &mut document,
            file_list: &documents_list_clone,
        };

        let template = parser_builder
            .parse(&render_ctx.this.markdown_body)
            .expect(&format!("parser_builder.parse failed on content result of current_file: {} post-frontmatter content: {:?}",render_ctx.this.file_path, &render_ctx.this.markdown_body));

        let render_ctx_obj = liquid::to_object(&render_ctx)?;
        let processed_markdown = template.render(&render_ctx_obj).unwrap();
        let markdown_html = djotters::markdown(&processed_markdown);

        println!("{:?}", render_ctx.this.file_path);
        let layout_for_document = if let Some(layout_in_cfg) = render_ctx
            .this
            .frontmatter
            .as_mapping()
            .and_then(|m| m.get("layout"))
            .and_then(|m| m.as_str())
        {
            layout_in_cfg.to_string()
        } else {
            find_template(
                config_struct.layouts.clone(),
                render_ctx.this.file_path.clone(),
            )?
        };

        render_ctx.this.markdown_processed = processed_markdown;
        render_ctx.this.html = markdown_html;
        let render_ctx_obj = liquid::to_object(&render_ctx)?;

        let document_as_html = parser_builder
            .parse_file(
                Path::new(root_dir.clone())
                    .join("_quickstatic/themes")
                    .join(&layout_for_document),
            )
            .and_then(|f| f.render(&render_ctx_obj))
            .expect(&format!(
                "document_as_html failed for file_path {:?} and layout {:?}",
                document.file_path, layout_for_document
            ));

        write_to_location(document.html_file_path, document_as_html.as_bytes())?;
    }

    Ok(())
}

fn write_to_location(file_path: String, data: &[u8]) -> eyre::Result<()> {
    // Make sure the destination directory exists
    if let Some(dir) = Path::new(&file_path).parent() {
        create_dir_all(dir)?;
    }

    let mut file = File::create(file_path)?;
    file.write_all(data)?;
    Ok(())
}

fn read_partials_from_directory(
    directory: &Path,
    extension: &str,
) -> io::Result<HashMap<String, String>> {
    let mut partials = HashMap::new();
    read_directory(directory, &mut partials, "", extension)?;
    Ok(partials)
}

fn read_directory(
    dir: &Path,
    partials: &mut HashMap<String, String>,
    prefix: &str,
    extension: &str,
) -> io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();

        // Check if it's a directory or a file with the desired extension
        if path.is_dir() {
            let new_prefix = format!("{}{}/", prefix, path.file_name().unwrap().to_string_lossy());
            read_directory(&path, partials, &new_prefix, extension)?;
        } else if path.extension().map_or(false, |ext| ext == extension) {
            let partial_name = format!(
                "{}{}.{}",
                prefix,
                path.file_stem().unwrap().to_string_lossy(),
                extension
            );
            let mut contents = String::new();
            fs::File::open(&path)?.read_to_string(&mut contents)?;
            partials.insert(partial_name, contents);
        }
    }
    Ok(())
}
