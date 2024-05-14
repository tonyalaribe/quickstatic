use gray_matter::{engine::YAML, Matter};
use indexmap::IndexMap;
use liquid_core::partials::{EagerCompiler, InMemorySource};
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
mod sort;
use base_cli::Commands;
use clap::Parser;
use djotters::Markdown;
use eyre::{eyre, WrapErr};
use notify_debouncer_mini::{new_debouncer, notify::*, DebounceEventResult};
use std::time::Duration;

#[derive(Clone, Debug, Serialize)]
struct DocumentData {
    file_path: String,
    file_destination_path: String,
    markdown_body: String,
    markdown_processed: String,
    content: String,
    toc: Vec<TOC>,
    frontmatter: Value,
    permalink: String,
}

#[derive(Clone, Debug, Serialize)]
struct TOC {
    level: usize,
    title: String,
    id: String,
}

// Config struct represents a key value tree of everything in the quickstatic config file.
// The quickstatic config file should be at: <static_site_>
#[derive(Deserialize, Serialize, Debug)]
struct Config {
    base_url: String,
    title: String,
    layouts: IndexMap<String, String>,
    ignore: Vec<String>,
    #[serde(skip_deserializing)]
    raw: Value,
}

#[derive(Debug, Serialize)]
struct RenderContext<'a> {
    config: &'a Config,
    this: &'a mut DocumentData,
    file_list: Vec<DocumentData>,
}

fn get_file_paths_recursive(
    config_struct: &Config,
    dir: &Path,
    exclude_dir_names: &Vec<&str>,
    extensions: &Vec<&str>,
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
                        extensions,
                    ));
                } else {
                    // If it's a file, add its path to the vector
                    if let Some(path_str) = path.to_str() {
                        for extension in extensions {
                            let ext_with_dot = ".".to_owned() + extension;

                            if path_str.to_string().ends_with(&ext_with_dot) {
                                paths.push(path_str.to_string());
                            }
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
) -> eyre::Result<()> {
    if src.is_dir() {
        if !dest.exists() {
            fs::create_dir_all(dest)
                .wrap_err(format!("copy_recursive failed to create {dest:?}"))?;
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
                        .wrap_err(format!("copy_recursive: failed to copy {path:?}"))?;
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

#[tokio::main]
async fn main() -> eyre::Result<()> {
    let cli_instance = base_cli::Cli::parse();

    match cli_instance.command {
        None => build_with_index(cli_instance.dir).await,
        Some(Commands::Build {}) => build_with_index(cli_instance.dir).await,
        Some(Commands::Serve { port }) => serve(cli_instance.dir, port).await,
    }
}

async fn build_with_index(root_dir: String) -> eyre::Result<()> {
    build(root_dir)?;

    // Generate pagefind's search index
     // let options = pagefind::SearchOptions {
    let options = pagefind::PagefindInboundConfig{
        source: "_quickstatic/public/".to_string(),
        site:  "_quickstatic/public/".to_string(),
        bundle_dir: None,
        output_subdir: None,
        output_path: None,
        root_selector: "html".into(),
        exclude_selectors: vec![],
        glob: "**/*.{html}".into(),
        force_language: None,
        serve: false,
        verbose: true,
        logfile: None,
        keep_index_url: false,
        service: false,
    };
    let search_options = pagefind::SearchOptions::load(options).unwrap();
    let runner = &mut pagefind::SearchState::new(search_options.clone());
    runner.log_start();
    _ = runner
        .fossick_many(search_options.site_source.clone(), search_options.glob.clone())
        .await;


    runner.build_indexes().await;

    Ok(())
}

async fn serve(dir: String, http_port: u16) -> eyre::Result<()> {
    println!(
        "Serving quickstatic at: https://localhost:{} and directory: {}\n\n",
        http_port, dir
    );

    // Run the directory watcher in a separate thread
    let dir_clone = dir.clone();
    std::thread::spawn(move || {
        match watch_directory_and_run_command(&dir_clone)
            .wrap_err("watch_directory_and_run_command error")
        {
            Err(e) => println!("Build Error: {:?}\n", e),
            Ok(_res) => println!("Rebuilt site \n"),
        };
    });

    let dir_to_serve = dir + "/_quickstatic/public/";
    let _ = serve_directory(http_port, dir_to_serve).await;
    Ok(())
}

fn watch_directory_and_run_command(dir: &str) -> eyre::Result<()> {
    match build(dir.to_string()) {
        Err(e) => println!("Build Error: {:?}\n", e),
        Ok(_res) => println!("Rebuilt site \n"),
    };

    let (tx, rx) = std::sync::mpsc::channel();

    println!("Watching and recompiling after every change");

    let mut debouncer = new_debouncer(
        Duration::from_millis(10),
        move |res: DebounceEventResult| {
            tx.send(res).unwrap();
        },
    )?;

    // Add a path to be watched. All files and directories at that path and
    // below will be monitored for changes.
    debouncer
        .watcher()
        .watch(Path::new("."), RecursiveMode::Recursive)?;

    for res in rx {
        match res {
            Ok(events) => events
                .iter()
                .filter(|f| !f.path.to_str().unwrap().contains("_quickstatic/public/"))
                .for_each(|_| match build(dir.to_string()) {
                    Err(e) => println!("Build Error: {:?}\n", e),
                    Ok(_res) => println!("Rebuilt site \n"),
                }),
            Err(e) => println!("Error {:?}", e),
        }
    }

    Ok(())
}

async fn serve_directory(port: u16, dir: String) -> eyre::Result<()> {
    let route = warp::fs::dir(dir);
    warp::serve(route).run(([127, 0, 0, 1], port)).await;
    Ok(())
}

fn find_template(layouts_map: IndexMap<String, String>, file_path: String) -> eyre::Result<String> {
    for (k, v) in &layouts_map {
        if glob_match::glob_match(&k, &file_path.clone()) {
            return Ok(v.into());
        };
    }

    Err(eyre::eyre!("expecting a general layout glob such as **/*.md to be set in the ./quickstatic.yaml config file: {:?} layout_map: {:?}", file_path, layouts_map))
}

fn build(root_dir: String) -> eyre::Result<()> {
    let dir = Path::new(&root_dir); // Specify the directory

    // Read the config into a config struct.
    let config_file_content = fs::read_to_string(dir.join("quickstatic.yaml"))
        .wrap_err("unable to find `quickstatic.yaml` config file")?;
    let config_value = serde_yaml::from_str::<Value>(&config_file_content)
        .wrap_err("unable to unmarshal config into serde yaml Value")?;
    let mut config_struct: Config = serde_yaml::from_value(config_value.clone())?;
    config_struct.raw = config_value;

    let exclude_dir_names = vec!["_quickstatic", ".git", "node_modules"];
    copy_recursive(
        &config_struct,
        dir,
        &exclude_dir_names,
        Path::new(&format!("{root_dir}/_quickstatic/public/")),
    )?;
    let file_paths = get_file_paths_recursive(
        &config_struct,
        dir,
        &exclude_dir_names,
        &vec!["md", "liquid"],
    );

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
        .filter(crate::where_glob::Ternary)
        .filter(crate::where_glob::StartsWith)
        .filter(crate::where_glob::Equals)
        .filter(crate::where_glob::Markdownify)
        .filter(crate::sort::Sort)
        .filter(liquid_lib::jekyll::Slugify)
        .filter(liquid_lib::jekyll::Push)
        .filter(liquid_lib::jekyll::Pop)
        .filter(liquid_lib::jekyll::Unshift)
        .filter(liquid_lib::jekyll::Shift)
        .filter(liquid_lib::jekyll::ArrayToSentenceString)
        .filter(liquid_lib::shopify::Pluralize);
    let parser_builder = builder.partials(partials_compiler).build()?;

    // documents_map holds the entire documents tree and can be iterated eg via prefix.
    let mut documents_list = vec![];

    for file_path in &file_paths {
        let file_path_str = file_path.to_string().to_owned();
        let file_path_no_root = file_path_str.strip_prefix(&root_dir).unwrap();
        let contents = fs::read_to_string(file_path.clone())?;

        // Any files with the liquid extension are stripped of the .liquid. The content is treated
        // as main content as is.
        let (file_content, frontmatter, file_destination_path) =
            if file_path_no_root.ends_with(".md") {
                let result = matter.parse(&contents);
                let frontmatter: Value = result
                    .data
                    .unwrap_or(gray_matter::Pod::Null)
                    .clone()
                    .deserialize()?;

                (
                    result.content.clone(),
                    frontmatter,
                    file_path_no_root.strip_suffix(".md").unwrap().to_owned() + ".html",
                )
            } else {
                (
                    contents,
                    serde_yaml::Value::Null,
                    file_path_no_root
                        .strip_suffix(".liquid")
                        .unwrap()
                        .to_owned(),
                )
            };
        let public_root_path = Path::new(&root_dir)
            .join("_quickstatic/public")
            .to_string_lossy()
            .to_owned()
            .to_string();
        let final_file_destination_path = format!("{}{}", public_root_path, file_destination_path);

        let document = DocumentData {
            file_path: file_path.clone(),
            file_destination_path: final_file_destination_path.clone(),
            markdown_body: file_content.clone(),
            markdown_processed: "".into(),
            frontmatter,
            content: file_content.into(),
            toc: vec![],
            permalink: final_file_destination_path
                .clone()
                .to_string()
                .trim_end_matches("index.html")
                .trim_start_matches("./_quickstatic/public")
                .to_owned(),
        };

        documents_list.push(document);

    }

    // Render all the markdowns and save them to final destination.
    let documents_list_clone = documents_list.clone();
    for document in &mut documents_list {
        let render_ctx = &mut RenderContext {
            config: &config_struct,
            this: document,
            file_list: documents_list_clone.clone(),
        };

        let render_ctx_obj = liquid::to_object(&render_ctx)?;
        if render_ctx.this.file_path.clone().ends_with(".md") {
            let template = parser_builder
                .parse(&render_ctx.this.markdown_body)
                .wrap_err(format!("parser_builder.parse template.render failed on current_file: {} post-frontmatter content: {:?}",render_ctx.this.file_path, &render_ctx.this.markdown_body))?;

            render_ctx.this.markdown_processed = template.render(&render_ctx_obj)?;
            let (md_processed, toc) = process_markdown(render_ctx.this.markdown_processed.clone())
                .wrap_err(format!("process_markdown: template.render failed on current_file: {} post-frontmatter content: {:?}",render_ctx.this.file_path, &render_ctx.this.markdown_body))?;
            render_ctx.this.content = md_processed;
            render_ctx.this.toc = toc;
        } else {
            let template = parser_builder
                .parse(&render_ctx.this.content)
                .wrap_err(format!("parser_builder.parse failed on content result of current_file: {} post-frontmatter content: {:?}",render_ctx.this.file_path, &render_ctx.this.markdown_body))?;

            render_ctx.this.content = template.render(&render_ctx_obj)
                .wrap_err(format!("parser_builder.parse template.render failed on current_file: {} post-frontmatter content: {:?}",render_ctx.this.file_path, &render_ctx.this.markdown_body))? ;
        }

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

        let render_ctx_obj = liquid::to_object(&render_ctx)?;
        let document_as_html = parser_builder
            .parse_file(
                Path::new(&root_dir)
                    .join("_quickstatic/themes")
                    .join(&layout_for_document),
            )
            .and_then(|f| f.render(&render_ctx_obj))
            .wrap_err(format!(
                "document_as_html failed for file_path {:?} and layout {:?}",
                render_ctx.this.file_path, layout_for_document
            ))?;

        write_to_location(
            render_ctx.this.file_destination_path.to_owned(),
            document_as_html.as_bytes(),
        )?;
    }

    Ok(())
}

fn process_markdown(md: String) -> eyre::Result<(String, Vec<TOC>)> {
    let (remaining_input, ast) = djotters::parse_markdown(&md)
        .map_err(|e| eyre!("{:#}", e).wrap_err("Failed to parse markdown"))?;
    
    if remaining_input != "" {
        return Err(eyre!("markdown parsing error. Had text remaining after parse: {}", remaining_input))
    }

    let headings: Vec<_> = ast
        .iter()
        .filter_map(|item| match item {
            Markdown::Heading(level, text, attrs) => {
                if *level > 1 {
                    Some(TOC {
                        level: level.to_owned(),
                        title: djotters::translator::translate_text(text.to_vec()),
                        id: attrs
                            .clone()
                            .unwrap()
                            .get("id")
                            .unwrap_or(&"".to_string())
                            .clone(),
                    })
                } else {
                    None
                }
            }
            _ => None,
        })
        .collect();
    let content = djotters::translate(ast);
    Ok((content, headings))
}

fn write_to_location(file_path: String, data: &[u8]) -> eyre::Result<()> {
    // Make sure the destination directory exists
    if let Some(dir) = Path::new(&file_path).parent() {
        create_dir_all(dir).wrap_err(format!(
            "write_to_location: create_dir_all failed for path: {}",
            &file_path
        ))?;
    }

    let mut file = File::create(&file_path)?;
    file.write_all(data).wrap_err(format!(
        "write_to_location: unable to write file data to file at path {}",
        &file_path
    ))?;
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
