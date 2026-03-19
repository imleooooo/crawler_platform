## Features

- **Browser Automation**: Uses `chromiumoxide` (Chrome DevTools Protocol) for controlling the browser.
- **High Performance**: Built on `tokio` for asynchronous execution.
- **Markdown Output**: Converts HTML to Markdown, optimized for LLM consumption.
- **Python Bindings**: Callable from Python using `pyo3`.

## Prerequisites

- Rust (install via `rustup` or `brew install rust`)
- Python 3.8+ (for bindings)

## Installation & Usage

### Rust

1. Build the project:

    ```bash
    cargo build --release
    ```

2. Run the example:

    ```bash
    cargo run
    ```

### Python

1. Create a virtual environment:

    ```bash
    python3 -m venv .venv
    source .venv/bin/activate
    ```

2. Install `maturin`:

    ```bash
    pip install maturin
    ```

3. Build and install the library:

    ```bash
    maturin develop
    maturin build --release
    ```

4. Run the test script:

    ```bash
    python test_binding.py
    ```

## License

[License Information]
