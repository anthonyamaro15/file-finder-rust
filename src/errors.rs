use thiserror::Error;
use std::io;

/// Main application error type that encompasses all possible errors
#[derive(Error, Debug)]
pub enum AppError {
    #[error("File system error: {0}")]
    FileSystem(#[from] io::Error),
    
    #[error("Configuration error: {message}")]
    Configuration { message: String },
    
    #[error("Path error: {message}")]
    Path { message: String },
    
    #[error("Image processing error: {message}")]
    Image { message: String },
    
    #[error("Terminal/UI error: {message}")]
    Terminal { message: String },
    
    #[error("Clipboard error: {message}")]
    Clipboard { message: String },
    
    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),
    
    #[error("IDE validation error: '{ide}' is not supported. Valid options: nvim, vscode, zed")]
    InvalidIDE { ide: String },
    
    #[error("Directory traversal error: {message}")]
    DirectoryTraversal { message: String },
    
    #[error("Cache error: {message}")]
    Cache { message: String },
}

/// Result type alias for convenience
pub type AppResult<T> = Result<T, AppError>;

impl AppError {
    /// Create a configuration error
    pub fn config<S: Into<String>>(message: S) -> Self {
        Self::Configuration { message: message.into() }
    }
    
    /// Create a path error
    pub fn path<S: Into<String>>(message: S) -> Self {
        Self::Path { message: message.into() }
    }
    
    /// Create an image processing error
    pub fn image<S: Into<String>>(message: S) -> Self {
        Self::Image { message: message.into() }
    }
    
    /// Create a terminal/UI error
    pub fn terminal<S: Into<String>>(message: S) -> Self {
        Self::Terminal { message: message.into() }
    }
    
    /// Create a clipboard error
    pub fn clipboard<S: Into<String>>(message: S) -> Self {
        Self::Clipboard { message: message.into() }
    }
    
    /// Create an IDE validation error
    pub fn invalid_ide<S: Into<String>>(ide: S) -> Self {
        Self::InvalidIDE { ide: ide.into() }
    }
    
    /// Create a directory traversal error
    pub fn directory_traversal<S: Into<String>>(message: S) -> Self {
        Self::DirectoryTraversal { message: message.into() }
    }
    
    /// Create a cache error
    pub fn cache<S: Into<String>>(message: S) -> Self {
        Self::Cache { message: message.into() }
    }
    
    /// Check if this error is recoverable
    pub fn is_recoverable(&self) -> bool {
        match self {
            AppError::FileSystem(_) => true,  // Can retry file operations
            AppError::Configuration { .. } => true,  // Can use defaults
            AppError::Cache { .. } => true,  // Can rebuild cache
            AppError::Clipboard { .. } => true,  // Can fall back to terminal output
            AppError::Image { .. } => true,  // Can skip image preview
            AppError::DirectoryTraversal { .. } => true,  // Can try different directory
            AppError::Path { .. } => false,  // Usually indicates corrupt data
            AppError::Terminal { .. } => false,  // Terminal issues are usually fatal
            AppError::Json(_) => false,  // Indicates corrupt config
            AppError::InvalidIDE { .. } => false,  // User input validation error
        }
    }
    
    /// Get a user-friendly message for display in UI
    pub fn user_message(&self) -> String {
        match self {
            AppError::FileSystem(e) => {
                match e.kind() {
                    io::ErrorKind::NotFound => "File or directory not found".to_string(),
                    io::ErrorKind::PermissionDenied => "Permission denied".to_string(),
                    io::ErrorKind::AlreadyExists => "File or directory already exists".to_string(),
                    _ => format!("File system error: {}", e),
                }
            },
            AppError::Configuration { message } => {
                format!("Configuration issue: {}", message)
            },
            AppError::Path { message } => {
                format!("Path issue: {}", message)
            },
            AppError::Image { message } => {
                format!("Cannot display image: {}", message)
            },
            AppError::Terminal { message } => {
                format!("Terminal error: {}", message)
            },
            AppError::Clipboard { message } => {
                format!("Clipboard error: {}", message)
            },
            AppError::Json(_) => {
                "Configuration file is corrupted. Using defaults.".to_string()
            },
            AppError::InvalidIDE { ide } => {
                format!("'{}' is not a supported editor. Use: nvim, vscode, or zed", ide)
            },
            AppError::DirectoryTraversal { message } => {
                format!("Cannot access directory: {}", message)
            },
            AppError::Cache { message } => {
                format!("Cache issue: {}", message)
            },
        }
    }
}

/// Helper trait to convert path-related operations to AppError
pub trait PathErrorExt<T> {
    fn path_context<S: Into<String>>(self, message: S) -> AppResult<T>;
}

impl<T> PathErrorExt<T> for Option<T> {
    fn path_context<S: Into<String>>(self, message: S) -> AppResult<T> {
        self.ok_or_else(|| AppError::path(message))
    }
}

impl<T, E> PathErrorExt<T> for Result<T, E> 
where 
    E: std::fmt::Display
{
    fn path_context<S: Into<String>>(self, message: S) -> AppResult<T> {
        self.map_err(|_| AppError::path(message))
    }
}
