use gray_matter::{engine::YAML, Matter};
use indexmap::IndexMap;
use liquid::ObjectView;
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
mod sort;
mod where_glob;
use base_cli::Commands;
use clap::Parser;
use eyre::WrapErr;
use jotdown::Container::Heading;

use jotdown::Event::Start;
use notify_debouncer_mini::{new_debouncer, notify::*, DebounceEventResult};
use std::time::Duration;

#[derive(Debug, Serialize)]
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
    // true under `serve`, so templates can skip e.g. analytics locally.
    #[serde(skip_deserializing)]
    dev: bool,
}

#[derive(Debug, liquid::ObjectView, liquid::ValueView)]
struct RenderContext<'a> {
    config: &'a liquid::model::Value,
    this: liquid::model::Value,
    file_list: &'a liquid::model::Value,
}

fn up_to_date(src: &Path, dest: &Path) -> bool {
    matches!((src.metadata(), dest.metadata()), (Ok(s), Ok(d)) if s.len() == d.len() && s.modified().ok() <= d.modified().ok())
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
                    && !up_to_date(&path, &new_dest)
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
    build(root_dir, false)?;

    // Generate pagefind's search index
    // let options = pagefind::SearchOptions {
    // let options = pagefind::PagefindInboundConfig{
    //     source: "_quickstatic/public/".to_string(),
    //     site:  "_quickstatic/public/".to_string(),
    //     bundle_dir: None,
    //     output_subdir: None,
    //     output_path: None,
    //     root_selector: "html".into(),
    //     exclude_selectors: vec![],
    //     glob: "**/*.{html}".into(),
    //     force_language: None,
    //     serve: false,
    //     verbose: true,
    //     logfile: None,
    //     keep_index_url: false,
    //     service: false,
    // };
    // let search_options = pagefind::SearchOptions::load(options).unwrap();
    // let runner = &mut pagefind::SearchState::new(search_options.clone());

    let options = pagefind::options::PagefindServiceConfig::builder()
        .keep_index_url(true)
        .force_language("en".to_string())
        .build();
    let mut runner =
        pagefind::api::PagefindIndex::new(Some(options)).expect("Options should be valid");
    let _ = runner
        .add_directory(
            "_quickstatic/public/".to_string(),
            Some("**/*.{html}".into()),
        )
        .await;

    // runner.log_start();
    // _ = runner
    //     .fossick_many(search_options.site_source.clone(), search_options.glob.clone())
    //     .await;

    let _ = runner.build_indexes().await;

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
    let rebuild = || {
        let started = std::time::SystemTime::now();
        match build(dir.to_string(), true) {
            Err(e) => println!("Build Error: {:?}\n", e),
            Ok(_res) => println!("Rebuilt site \n"),
        };
        started
    };
    let mut built_at = rebuild();

    let (tx, rx) = std::sync::mpsc::channel();
    let mut debouncer = new_debouncer(Duration::from_millis(250), tx)?;
    debouncer
        .watcher()
        .watch(Path::new(dir), RecursiveMode::Recursive)?;
    println!("Watching and recompiling after every change");

    // FSEvents may re-report a change the previous build already saw, so files untouched since then are skipped.
    let relevant = |built_at, res: DebounceEventResult| match res {
        Ok(events) => events.iter().any(|e| {
            !e.path.to_string_lossy().contains("_quickstatic/public")
                && !e.path.components().any(|c| {
                    [".git", "node_modules"].contains(&c.as_os_str().to_str().unwrap_or_default())
                })
                && fs::metadata(&e.path)
                    .ok()
                    .and_then(|m| [m.modified(), m.created()].into_iter().flatten().max())
                    .map_or(true, |t| t >= built_at)
        }),
        Err(e) => {
            println!("Error {:?}", e);
            false
        }
    };
    // Coalesce everything queued (e.g. during a build) into a single rebuild.
    while let Ok(res) = rx.recv() {
        if std::iter::once(res)
            .chain(rx.try_iter())
            .map(|r| relevant(built_at, r))
            .fold(false, |a, b| a | b)
        {
            built_at = rebuild();
        }
    }
    Ok(())
}

async fn serve_directory(port: u16, dir: String) -> eyre::Result<()> {
    let route = warp::fs::dir(dir);
    warp::serve(route).run(([127, 0, 0, 1], port)).await;
    Ok(())
}

fn find_template(layouts_map: &IndexMap<String, String>, file_path: &str) -> eyre::Result<String> {
    for (k, v) in layouts_map {
        if glob_match::glob_match(k, file_path) {
            return Ok(v.into());
        };
    }

    Err(eyre::eyre!("expecting a general layout glob such as **/*.md to be set in the ./quickstatic.yaml config file: {:?} layout_map: {:?}", file_path, layouts_map))
}

fn build(root_dir: String, dev: bool) -> eyre::Result<()> {
    let dir = Path::new(&root_dir); // Specify the directory

    // Read the config into a config struct.
    let config_file_content = fs::read_to_string(dir.join("quickstatic.yaml"))
        .wrap_err("unable to find `quickstatic.yaml` config file")?;
    let config_value = serde_yaml::from_str::<Value>(&config_file_content)
        .wrap_err("unable to unmarshal config into serde yaml Value")?;
    let mut config_struct: Config = serde_yaml::from_value(config_value.clone())?;
    config_struct.raw = config_value;
    config_struct.dev = dev;

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
        .filter(crate::where_glob::Date)
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
                let result = matter.parse(&contents)?;
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
    let config_val = liquid::model::to_value(&config_struct)?;
    let file_list = liquid::model::to_value(&documents_list)?;
    let ctx = |doc: &DocumentData| -> liquid_core::Result<RenderContext> {
        Ok(RenderContext {
            config: &config_val,
            this: liquid::model::to_value(doc)?,
            file_list: &file_list,
        })
    };
    let mut layouts: HashMap<String, liquid::Template> = HashMap::new();
    for doc in &mut documents_list {
        let rendered = parser_builder
            .parse(&doc.markdown_body)
            .and_then(|t| t.render(&ctx(doc)?))
            .wrap_err_with(|| format!("liquid render failed on current_file: {}", doc.file_path))?;
        if doc.file_path.ends_with(".md") {
            (doc.content, doc.toc) = process_markdown(&rendered);
            doc.markdown_processed = rendered;
        } else {
            doc.content = rendered;
        }

        let layout = match doc.frontmatter.get("layout").and_then(|m| m.as_str()) {
            Some(l) => l.to_string(),
            None => find_template(&config_struct.layouts, &doc.file_path)?,
        };
        let template = match layouts.entry(layout) {
            std::collections::hash_map::Entry::Occupied(e) => e.into_mut(),
            std::collections::hash_map::Entry::Vacant(e) => {
                let t = parser_builder
                    .parse_file(
                        Path::new(&root_dir)
                            .join("_quickstatic/themes")
                            .join(e.key()),
                    )
                    .wrap_err_with(|| format!("failed to parse layout {:?}", e.key()))?;
                e.insert(t)
            }
        };
        let document_as_html = template.render(&ctx(doc)?).wrap_err_with(|| {
            format!("document_as_html failed for file_path {:?}", doc.file_path)
        })?;

        write_to_location(
            doc.file_destination_path.to_owned(),
            document_as_html.as_bytes(),
        )?;
    }

    Ok(())
}

fn process_markdown(md: &str) -> (String, Vec<TOC>) {
    let events = jotdown::Parser::new(md);
    let hds = events
        .clone()
        .filter_map(|item| match item {
            Start(
                Heading {
                    level,
                    has_section: _,
                    id,
                },
                _,
            ) => {
                if level > 1 {
                    Some(TOC {
                        level: level.into(),
                        title: id.clone().into_owned().replace("-", " "),
                        id: id.into_owned(),
                    })
                } else {
                    None
                }
            }

            _ => None,
        })
        .collect();
    (jotdown::html::render_to_string(events), hds)
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
