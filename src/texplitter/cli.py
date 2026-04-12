import argparse


def main():
    parser = argparse.ArgumentParser(description="texplitter command line tool")
    parser.add_argument(
        "command",
        choices=["config"],
        help="Command to execute. Use 'config' to start the FastAPI editor."
    )
    parser.add_argument(
        "--host",
        type=str,
        default="127.0.0.1",
        help="Host address to run the FastAPI editor. Default is '127.0.0.1'."
    )
    parser.add_argument(
        "--port",
        type=int,
        default=8000,
        help="Port number to run the FastAPI editor. Default is 8000."
    )

    args = parser.parse_args()

    if args.command == "config":
        start_config_editor(args.host, args.port)


def start_config_editor(host, port):
    import multiprocessing
    import time
    import webbrowser

    import requests

    multiprocessing.set_start_method('spawn', force=True)

    print(f"Starting the FastAPI editor on {host}:{port}...")

    server_process = multiprocessing.Process(target=run_server, args=(host, port))
    server_process.start()

    url = f"http://{host}:{port}"
    while True:
        try:
            response = requests.get(url)
            if response.status_code == 200:
                break
        except requests.exceptions.ConnectionError:
            pass
        time.sleep(1)

    webbrowser.open(url)
    server_process.join()


def run_server(host, port):
    import uvicorn
    uvicorn.run("texplitter_config_editor.api.main:app", host=host, port=port)
