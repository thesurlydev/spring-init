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
    /// Suggest dependencies based on PRD
    SuggestDeps {
        /// Path to PRD file
        #[arg(long)]
        prd: String,
    },
}

#[derive(Serialize, Deserialize)]
struct ProjectConfig {
    app_name: String,
    package_name: String,
    version: String,
    projects_dir: String,
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
        Commands::SuggestDeps { prd } => suggest_dependencies(&prd).await?,
    }

    Ok(())
}

fn show_info(config: &ProjectConfig) {
    println!("APP_NAME: {}", config.app_name);
    println!("PACKAGE_NAME: {}", config.package_name);
    println!("ARTIFACT_NAME: {}", config.app_name);
    println!("VERSION: {}", config.version);
    println!("PROJECTS_DIR: {}", config.projects_dir);
    println!("APP_DIR: {}", config.app_dir().display());
    println!("JAR_PATH: {}", config.jar_path().display());
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
        "https://start.spring.io/starter.zip?type=maven-project&language=java&bootVersion=3.4.2&baseDir={}&groupId={}&artifactId={}&name={}&packageName={}&packaging=jar&javaVersion=21&dependencies={}",
        config.app_name, config.package_name, config.app_name, config.app_name, config.package_name, all_deps.trim()
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
