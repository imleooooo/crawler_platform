use crate::config::BrowserConfig;
use crate::crawler::AsyncWebCrawler;
use crate::errors::CrawlError;
use pyo3::create_exception;
use pyo3::exceptions::PyException;
use pyo3::prelude::*;
use pyo3::types::PyDict;
use std::sync::Arc;
use tokio::runtime::Runtime;

create_exception!(lab_crawl, CrawlErrorPy, PyException);
create_exception!(lab_crawl, BrowserLaunchError, CrawlErrorPy);
create_exception!(lab_crawl, NavigationError, CrawlErrorPy);
create_exception!(lab_crawl, ElementNotFound, CrawlErrorPy);
create_exception!(lab_crawl, TimeoutError, CrawlErrorPy);
create_exception!(lab_crawl, JsError, CrawlErrorPy);
create_exception!(lab_crawl, ScreenshotError, CrawlErrorPy);

impl From<CrawlError> for PyErr {
    fn from(err: CrawlError) -> PyErr {
        match err {
            CrawlError::BrowserLaunchError(msg) => BrowserLaunchError::new_err(msg),
            CrawlError::NavigationError(msg) => NavigationError::new_err(msg),
            CrawlError::ElementNotFound(msg) => ElementNotFound::new_err(msg),
            CrawlError::Timeout(msg) => TimeoutError::new_err(msg),
            CrawlError::JsError(msg) => JsError::new_err(msg),
            CrawlError::ScreenshotError(msg) => ScreenshotError::new_err(msg),
            CrawlError::IoError(e) => pyo3::exceptions::PyIOError::new_err(e.to_string()),
            CrawlError::Other(msg) => CrawlErrorPy::new_err(msg),
            CrawlError::CloseFailedWithResult { close_error, .. } => {
                CrawlErrorPy::new_err(format!("page.close() failed: {}", close_error))
            }
        }
    }
}

#[pyclass]
struct Crawl4AiRs {
    inner: Arc<AsyncWebCrawler>,
    rt: Arc<Runtime>,
}

#[pymethods]
impl Crawl4AiRs {
    #[new]
    #[pyo3(signature = (headless=true, user_agent=None, rotate_user_agent=false, disable_images=false, disable_css=false, semaphore_size=None))]
    fn new(
        headless: bool,
        user_agent: Option<String>,
        rotate_user_agent: bool,
        disable_images: bool,
        disable_css: bool,
        semaphore_size: Option<usize>,
    ) -> PyResult<Self> {
        let mut config = BrowserConfig {
            headless,
            disable_images,
            disable_css,
            rotate_user_agent,
            semaphore_size,
            ..BrowserConfig::default()
        };
        if let Some(ua) = user_agent {
            config.user_agent = Some(ua);
        }

        let rt = Runtime::new()
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?;
        let crawler = rt.block_on(async { AsyncWebCrawler::new(config).await })?;

        Ok(Crawl4AiRs {
            inner: Arc::new(crawler),
            rt: Arc::new(rt),
        })
    }

    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature = (url, magic_markdown=false, run_mode=None, api_key=None, model=None, prompt=None, ignore_links=false))]
    fn crawl(
        &self,
        py: Python,
        url: String,
        magic_markdown: bool,
        run_mode: Option<String>,
        api_key: Option<String>,
        model: Option<String>,
        prompt: Option<String>,
        ignore_links: bool,
    ) -> PyResult<Py<PyAny>> {
        let inner = self.inner.clone();

        // Block on the async call, errors are converted to PyErr via ?
        let run_config = crate::config::CrawlerRunConfig {
            url: url.clone(),
            magic_markdown,
            run_mode,
            api_key,
            model,
            prompt,
            ignore_links,
            ..crate::config::CrawlerRunConfig::default()
        };

        let result = self
            .rt
            .block_on(async move { inner.arun(&url, Some(run_config)).await })?;

        let dict = PyDict::new(py);
        dict.set_item("url", result.url)?;
        dict.set_item("html", result.html)?;
        dict.set_item("markdown", result.markdown)?;
        dict.set_item("success", result.success)?;
        if let Some(err) = result.error_message {
            dict.set_item("error_message", err)?;
        }

        Ok(dict.into())
    }

    #[pyo3(signature = (urls, magic_markdown=false, run_mode=None, ignore_links=false))]
    fn crawl_many(
        &self,
        py: Python,
        urls: Vec<String>,
        magic_markdown: bool,
        run_mode: Option<String>,
        ignore_links: bool,
    ) -> PyResult<Vec<Py<PyAny>>> {
        let inner = self.inner.clone();

        let results = self.rt.block_on(async move {
            let run_config = crate::config::CrawlerRunConfig {
                magic_markdown,
                run_mode,
                ignore_links,
                ..crate::config::CrawlerRunConfig::default()
            };

            inner.arun_many(urls, Some(run_config)).await
        });

        let mut py_results = Vec::new();
        for result in results {
            let dict = PyDict::new(py);
            dict.set_item("url", result.url)?;
            dict.set_item("html", result.html)?;
            dict.set_item("markdown", result.markdown)?;
            dict.set_item("success", result.success)?;
            if let Some(err) = result.error_message {
                dict.set_item("error_message", err)?;
            }
            py_results.push(dict.into());
        }

        Ok(py_results)
    }

    fn close(&self) -> PyResult<()> {
        let inner = self.inner.clone();
        let rt = self.rt.clone();

        rt.block_on(async move {
            inner.close().await;
        });
        Ok(())
    }
}

#[pymodule]
fn lab_crawl(py: Python, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Crawl4AiRs>()?;
    m.add("CrawlError", py.get_type::<CrawlErrorPy>())?;
    m.add("BrowserLaunchError", py.get_type::<BrowserLaunchError>())?;
    m.add("NavigationError", py.get_type::<NavigationError>())?;
    m.add("ElementNotFound", py.get_type::<ElementNotFound>())?;
    m.add("TimeoutError", py.get_type::<TimeoutError>())?;
    m.add("JsError", py.get_type::<JsError>())?;
    m.add("ScreenshotError", py.get_type::<ScreenshotError>())?;
    Ok(())
}
