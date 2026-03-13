import * as vscode from "vscode";
import {
  LanguageClient,
  LanguageClientOptions,
  ServerOptions,
  Executable,
} from "vscode-languageclient/node";

let client: LanguageClient | undefined;

export function activate(context: vscode.ExtensionContext) {
  const config = vscode.workspace.getConfiguration("zenscript");
  const serverPath = config.get<string>("serverPath", "zsc");

  const serverOptions: ServerOptions = {
    run: { command: serverPath, args: ["lsp"] } as Executable,
    debug: { command: serverPath, args: ["lsp"] } as Executable,
  };

  const clientOptions: LanguageClientOptions = {
    documentSelector: [{ scheme: "file", language: "zenscript" }],
    synchronize: {
      fileEvents: vscode.workspace.createFileSystemWatcher("**/*.zs"),
    },
  };

  client = new LanguageClient(
    "zenscript",
    "ZenScript Language Server",
    serverOptions,
    clientOptions
  );

  client.start();
}

export function deactivate(): Thenable<void> | undefined {
  if (!client) {
    return undefined;
  }
  return client.stop();
}
