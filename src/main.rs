use liquid::{ObjectView, model::ValueView};
use prose;
use serde::{Serialize, Deserialize};
use std::{fs::{self, File, create_dir_all}, path::Path, io::Write};
use gray_matter::Matter;
use gray_matter::engine::YAML;
use serde_yaml::Value;
use eyre::{WrapErr, Result};
mod base_cli;
use base_cli::Commands;
use clap::Parser;


#[derive(Debug, Serialize)]
struct DocumentData {
    file_path: String,
    markdown_raw: String, 
    markdown_processed: String,
    html: String,
    frontmatter: Value,
}

// Config struct represents a key value tree of everything in the quickstatic config file. 
// The quickstatic config file should be at: <static_site_>
#[derive(Deserialize, Serialize, Debug)]
struct Config {
    base_url: String,
    title: String,
    theme: String,
    #[serde(skip_deserializing)]
    raw: Value,
}

#[derive(Debug, Serialize)]
struct RenderContext<'a > {
    config: &'a Config,
    this: &'a mut DocumentData,
}

fn get_file_paths_recursive(dir: &Path, exclude_dir_name: &str) -> Vec<String> {
    let mut paths = Vec::new();

    if dir.is_dir() {
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    if path.file_name().unwrap_or_default() == exclude_dir_name {
                        continue; // Skip this directory and continue with the next entry
                    }
                    // If it's a directory, recursively get the files within it
                    paths.extend(get_file_paths_recursive(&path, exclude_dir_name));
                } else {
                    // If it's a file, add its path to the vector
                    if let Some(path_str) = path.to_str() {
                        paths.push(path_str.to_string());
                    }
                }
            }
        }
    }

    paths
}

fn main() -> eyre::Result<()> {
    let cli_instance = base_cli::Cli::parse();

    match cli_instance.command {
        None => build("./"),
        Some(Commands::Build {dir}) => build(&dir),
    }
}

fn build(root_dir: &str) -> eyre::Result<()> {
    let dir = Path::new(root_dir); // Specify the directory
    

    // Read the config into a config struct. 
    let config_file_content = fs::read_to_string(dir.join("_quickstatic/config.yaml"))?;
    let config_value = serde_yaml::from_str::<Value>(&config_file_content).unwrap();
    let mut config_struct:Config = serde_yaml::from_value(config_value.clone())?;
    config_struct.raw = config_value;

    let exclude_dir_name = "_quickstatic";
    let file_paths = get_file_paths_recursive(dir, exclude_dir_name);

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

    for file_path in file_paths.clone() {
        let file_path_str = file_path.to_string().to_owned();
        let file_path_no_root = file_path_str.strip_prefix(root_dir).unwrap(); 

        println!("file_path1: {file_path_str:?} {file_path_no_root:?}");

        let contents = fs::read_to_string(file_path.clone())?;
        let result = matter.parse(&contents);
        let frontmatter:Value = result.data.unwrap_or(gray_matter::Pod::Null).clone().deserialize()?;

        let mut document  = &mut DocumentData {
            file_path: file_path.clone(),
            markdown_raw: result.content.clone(),
            markdown_processed: "".into(),
            frontmatter: frontmatter.clone(),
            html: "".into(),
        };
        let mut render_ctx = &mut RenderContext{
            config: &config_struct, 
            this: &mut document, 
        };

        // TODO: do this in a new loop, so the context can contain the entire render tree, to
        // support referencing other documents in the template. Eg in table of content pages.
        // or listing categories and tags.
        let parser_builder = liquid::ParserBuilder::with_stdlib()
            .build().unwrap();

        let template = parser_builder.parse(&result.content).unwrap();

        let render_ctx_obj = liquid::to_object(&render_ctx)?;

        let processed_markdown = template.render(&render_ctx_obj).unwrap();
        let markdown_html = prose::markdown(&processed_markdown);

        println!("{file_path}, {processed_markdown}");

        let layout_for_document:&str = frontmatter
            .as_mapping()
            .unwrap()
            .get("layout")
            .unwrap()
            .as_str()
            .unwrap();

        render_ctx.this.markdown_processed = processed_markdown;
        render_ctx.this.html = markdown_html;
        let render_ctx_obj = liquid::to_object(&render_ctx)?;
        
        let document_as_html = parser_builder
            .parse_file(Path::new(root_dir.clone()).join("_quickstatic/themes").join(layout_for_document ))
            .unwrap()
            .render(&render_ctx_obj)
            .unwrap();


        let html_file_path  = file_path_no_root.strip_suffix(".md").unwrap().to_owned() + ".html";
        let public_root_path = Path::new(root_dir.clone())
            .join("_quickstatic/public").to_string_lossy().to_owned().to_string();
        let final_html_file_path = format!("{}{}", public_root_path, html_file_path);

         
        // Make sure the destination directory exists
        if let Some(dir) = Path::new(&final_html_file_path).parent() {
            create_dir_all(dir)?;
        }

        let mut file = File::create(final_html_file_path)?;
        file.write_all(document_as_html.as_bytes())?;
    }

    Ok(())
}
