"""Thunderus - Terminal AI Assistant Web Interface"""

from flask import Flask, render_template
import argparse

app = Flask(__name__)


@app.route("/")
def welcome():
    return render_template("welcome.html")


@app.route("/tutorial")
def tutorial():
    return render_template("tutorial.html")


@app.route("/chat-active")
def chat_active():
    return render_template("chat-active.html")


@app.route("/tools")
def tools():
    return render_template("tools.html")


@app.route("/files")
def files():
    return render_template("files.html")


@app.route("/loading")
def loading():
    return render_template("loading.html")


@app.route("/settings")
def settings():
    return render_template("settings.html")


@app.route("/help")
def help_page():
    return render_template("help.html")


def main():
    parser = argparse.ArgumentParser(description="Thunderus Web Interface")
    parser.add_argument(
        "--port", type=int, default=5000, help="Port to run the server on"
    )
    args = parser.parse_args()
    app.run(debug=True, port=args.port)


if __name__ == "__main__":
    main()
