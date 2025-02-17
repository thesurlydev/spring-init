use clap::{Parser, Subcommand};
use color_eyre::eyre::Result;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
mod claude;

#[derive(Parser)]
#[command(name = "spring-init")]
#[command(about = "Create and manage Spring Boot projects", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Display project information
    Info,
    /// Reset the project state
    Reset,
    /// Initialize a new Spring Boot project
    Init {
        /// Path to PRD file for automatic dependency selection
        #[arg(long)]
        prd: Option<String>,
        /// Additional dependencies to always include
        #[arg(long, value_delimiter = ',')]
        include: Option<Vec<String>>,
    },
    /// Build the project
    Build,
    /// Run the project
    Run,
    /// List all available dependency IDs
    Deps,
    /// Suggest dependencies based on PRD
    SuggestDeps {
        /// Path to PRD file
        #[arg(long)]
        prd: String,
    },
}

#[derive(Serialize, Deserialize)]
struct ProjectConfig {
    boot_version: String,
    java_version: String,
    app_name: String,
    package_name: String,
    version: String,
    projects_dir: String,
    maven_plugins: Vec<String>,
}

impl ProjectConfig {
    fn new() -> Result<Self> {
        let config_str = fs::read_to_string("config.json")?;
        let config: ProjectConfig = serde_json::from_str(&config_str)?;
        Ok(config)
    }

    fn app_dir(&self) -> PathBuf {
        PathBuf::from(&self.projects_dir).join(&self.app_name)
    }

    fn jar_path(&self) -> PathBuf {
        self.app_dir()
            .join("target")
            .join(format!("{}-{}.jar", self.app_name, self.version))
    }
}

async fn suggest_dependencies(prd_path: &str) -> Result<()> {
    // Read the PRD file
    let prd_content = fs::read_to_string(prd_path)?;
    
    // Read the dependencies metadata
    let deps_content = fs::read_to_string("client.json")?;
    let deps: serde_json::Value = serde_json::from_str(&deps_content)?;
    
    // Create a system prompt that includes the dependencies data
    let system_prompt = format!(
        "You are an expert in Spring Boot applications. Your task is to analyze a PRD (Product Requirements Document) \
        and suggest the most appropriate Spring Boot dependencies from the available options. Here is the list of \
        available dependencies with their descriptions:\n\n{}\n\nAnalyze the following PRD and respond with a list \
        of recommended dependency IDs, along with a brief explanation of why each dependency is needed. Only include \
        dependencies that are directly relevant to the requirements.",
        serde_json::to_string_pretty(&deps["dependencies"]["values"])?
    );
    
    // Initialize Claude client
    let claude = claude::ClaudeClient::new()?;
    
    // Get dependency suggestions
    let response = claude.send_message(&system_prompt, &prd_content).await?;
    println!("{}", response);
    
    Ok(())
}

async fn list_dependencies() -> Result<()> {
    println!("Fetching available dependencies from start.spring.io...");
    let client = reqwest::Client::new();
    let response = client
        .get("https://start.spring.io/metadata/client")
        .send()
        .await
        .map_err(|e| color_eyre::eyre::eyre!("Failed to fetch dependencies: {}", e))?
        .json::<serde_json::Value>()
        .await
        .map_err(|e| color_eyre::eyre::eyre!("Failed to parse response: {}", e))?;

    let mut dep_list: Vec<(String, String)> = Vec::new();

    // Process nested dependencies
    if let Some(categories) = response["dependencies"]["values"].as_array() {
        for category in categories {
            if let Some(deps) = category["values"].as_array() {
                for dep in deps {
                    if let (Some(id), Some(name), Some(description)) = (
                        dep["id"].as_str(),
                        dep["name"].as_str(),
                        dep["description"].as_str(),
                    ) {
                        dep_list.push((id.to_string(), format!("{} - {}", name, description)));
                    }
                }
            }
        }
    }

    // Sort by ID
    dep_list.sort_by(|a, b| a.0.cmp(&b.0));

    // Sort dependencies by ID
    dep_list.sort_by(|a, b| a.0.cmp(&b.0));

    // Print in a formatted table
    println!("Available Spring Boot Dependencies\n");
    println!("{:<40} {}", "ID", "Description");
    println!("{:-<120}", "");

    for (id, desc) in dep_list {
        // Wrap description text
        let wrapped_desc = textwrap::fill(&desc, 70);
        let mut lines = wrapped_desc.lines();
        
        if let Some(first_line) = lines.next() {
            println!("{:<40} {}", id, first_line);
            for line in lines {
                println!("{:<40} {}", "", line);
            }
        }
    }

    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install()?;
    let cli = Cli::parse();
    let config = ProjectConfig::new()?;

    match cli.command {
        Commands::Info => show_info(&config),
        Commands::Reset => reset(&config)?,
        Commands::Init { prd, include } => init_project(&config, prd.as_deref(), include).await?,
        Commands::Build => build_project(&config)?,
        Commands::Run => run_project(&config)?,
        Commands::Deps => list_dependencies().await?,
        Commands::SuggestDeps { prd } => suggest_dependencies(&prd).await?,
    }

    Ok(())
}

fn show_info(config: &ProjectConfig) {
    println!("     APP NAME: {}", config.app_name);
    println!(" PACKAGE NAME: {}", config.package_name);
    println!("ARTIFACT NAME: {}", config.app_name);
    println!("      VERSION: {}", config.version);
    println!(" BOOT VERSION: {}", config.boot_version);
    println!(" JAVA VERSION: {}", config.java_version);
    println!(" PROJECTS DIR: {}", config.projects_dir);
    println!("      APP DIR: {}", config.app_dir().display());
    println!("     JAR PATH: {}", config.jar_path().display());
}

fn reset(config: &ProjectConfig) -> Result<()> {
    // Remove spring.zip if it exists
    if Path::new("spring.zip").exists() {
        fs::remove_file("spring.zip")?;
    }

    // Remove app directory if it exists
    if config.app_dir().exists() {
        fs::remove_dir_all(config.app_dir())?;
    }

    println!("Project reset complete");
    Ok(())
}

async fn init_project(config: &ProjectConfig, prd_path: Option<&str>, include: Option<Vec<String>>) -> Result<()> {
    // Get dependencies from PRD if provided
    let mut all_deps = if let Some(prd_path) = prd_path {
        // Read the PRD file
        let prd_content = fs::read_to_string(prd_path)?;
        
        // Read the dependencies metadata
        let deps_content = fs::read_to_string("client.json")?;
        let deps: serde_json::Value = serde_json::from_str(&deps_content)?;
        
        // Create a system prompt that includes the dependencies data
        let system_prompt = format!(
            "You are an expert in Spring Boot applications. Your task is to analyze a PRD (Product Requirements Document) \
            and suggest the most appropriate Spring Boot dependencies from the available options. Here is the list of \
            available dependencies with their descriptions:\n\n{}\n\nAnalyze the following PRD and respond ONLY with a \
            comma-separated list of dependency IDs. Do not include any explanations or other text.",
            serde_json::to_string_pretty(&deps["dependencies"]["values"])?
        );
        
        // Initialize Claude client
        let claude = claude::ClaudeClient::new()?;
        
        // Get dependency suggestions
        claude.send_message(&system_prompt, &prd_content).await?
    } else {
        String::from("web")
    };

    // Add included dependencies
    if let Some(included) = include {
        let prd_deps: Vec<&str> = all_deps.split(',').map(|s| s.trim()).collect();
        let mut combined_deps: Vec<String> = prd_deps.iter().map(|&s| s.to_string()).collect();
        combined_deps.extend(included);
        combined_deps.sort();
        combined_deps.dedup();
        all_deps = combined_deps.join(",");
    };

    // First reset
    reset(config)?;

    // Download Spring Boot scaffold
    let url = format!(
        "https://start.spring.io/starter.zip?type=maven-project&language=java&bootVersion={}&baseDir={}&groupId={}&artifactId={}&name={}&packageName={}&packaging=jar&javaVersion={}&version={}&dependencies={}",
        config.boot_version, config.app_name, config.package_name, config.app_name, config.app_name, config.package_name, config.java_version, config.version, all_deps.trim()
    );

    println!("Using dependencies: {}", all_deps.trim());
    println!("Full URL: {}", url);

    println!("Downloading Spring Boot scaffold...");
    let status = Command::new("curl")
        .arg(url)
        .arg("-o")
        .arg("spring.zip")
        .status()?;

    if !status.success() {
        return Err(color_eyre::eyre::eyre!(
            "Failed to download Spring Boot scaffold"
        ));
    }

    // Unzip the scaffold
    println!("Unzipping Spring Boot scaffold...");
    let status = Command::new("unzip")
        .arg("spring.zip")
        .arg("-d")
        .arg(&config.projects_dir)
        .status()?;

    if !status.success() {
        return Err(color_eyre::eyre::eyre!(
            "Failed to unzip Spring Boot scaffold"
        ));
    }

    // Clean up zip file
    fs::remove_file("spring.zip")?;

    // Get project version from pom.xml using Maven
    let output = Command::new("./mvnw")
        .current_dir(&config.app_dir())
        .arg("help:evaluate")
        .arg("-Dexpression=project.version")
        .arg("-q")
        .arg("-DforceStdout")
        .output()?;

    if !output.status.success() {
        return Err(color_eyre::eyre::eyre!("Failed to get project version from pom.xml"));
    }

    // Sync plugins from config.json to pom.xml
    sync_plugins(config)?;

    println!("Project initialization complete");
    Ok(())
}

fn build_project(config: &ProjectConfig) -> Result<()> {
    println!("Building project...");
    let status = Command::new("mvn")
        .arg("package")
        .current_dir(config.app_dir())
        .status()?;

    if !status.success() {
        return Err(color_eyre::eyre::eyre!("Failed to build project"));
    }

    println!("Build complete");
    Ok(())
}

fn sync_plugins(config: &ProjectConfig) -> Result<()> {
    // Read existing pom.xml content
    let pom_path = config.app_dir().join("pom.xml");
    let pom_content = fs::read_to_string(&pom_path)?;

    // For each plugin in config.json
    for plugin in &config.maven_plugins {
        // Check if plugin is already in pom.xml
        if !pom_content.contains(plugin) {
            println!("Adding plugin: {}", plugin);

            // Extract group:artifact:version from plugin string
            let parts: Vec<&str> = plugin.split(":").collect();
            if parts.len() != 3 {
                return Err(color_eyre::eyre::eyre!("Invalid plugin format: {}", plugin));
            }

            // Extract plugin coordinates
            let parts: Vec<&str> = plugin.split(":").collect();
            let (group_id, artifact_id, version) = (
                parts[0], parts[1], parts[2]
            );

            // Read current pom.xml
            let mut pom_content = fs::read_to_string(&pom_path)?;

            // Check if build and plugins sections exist, if not add them
            if !pom_content.contains("<build>") {
                let insert_pos = pom_content.find("</project>").ok_or_else(|| 
                    color_eyre::eyre::eyre!("Could not find </project> tag in pom.xml"))?;
                pom_content.insert_str(insert_pos, "
    <build>
        <plugins>
        </plugins>
    </build>
");
            } else if !pom_content.contains("<plugins>") {
                let insert_pos = pom_content.find("</build>").ok_or_else(|| 
                    color_eyre::eyre::eyre!("Could not find </build> tag in pom.xml"))?;
                pom_content.insert_str(insert_pos, "
        <plugins>
        </plugins>
");
            }

            // Add plugin configuration
            let plugin_xml = format!("
            <plugin>
                <groupId>{}</groupId>
                <artifactId>{}</artifactId>
                <version>{}</version>
            </plugin>", group_id, artifact_id, version);

            let plugins_end_pos = pom_content.find("</plugins>").ok_or_else(|| 
                color_eyre::eyre::eyre!("Could not find </plugins> tag in pom.xml"))?;
            pom_content.insert_str(plugins_end_pos, &plugin_xml);

            // Write updated pom.xml
            fs::write(&pom_path, pom_content)?;
        }
    }

    Ok(())
}

fn run_project(config: &ProjectConfig) -> Result<()> {
    // First build the project
    build_project(config)?;

    println!("Running project...");
    let status = Command::new("java")
        .arg("-jar")
        .arg(config.jar_path())
        .status()?;

    if !status.success() {
        return Err(color_eyre::eyre::eyre!("Failed to run project"));
    }

    Ok(())
}
