use std::io;

/// Opens the sign-in URL in the user's default browser.
pub trait BrowserLauncher: Send + Sync {
    fn open(&self, url: &str) -> io::Result<()>;
}

/// Hands the URL to the shell. Steam credentials are typed into the real browser, in a window this
/// process cannot read -- which is the entire reason the flow is not an embedded webview.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemBrowserLauncher;

impl BrowserLauncher for SystemBrowserLauncher {
    fn open(&self, url: &str) -> io::Result<()> {
        open::that_detached(url)
    }
}
