// SPDX-License-Identifier: Apache-2.0 OR MIT

import * as vscode from "vscode";
import {
  LanguageClient,
  LanguageClientOptions,
  ServerOptions,
  TransportKind,
} from "vscode-languageclient/node";

let client: LanguageClient | undefined;

/** Start `bastion lsp` for .bcl files and keep it running until deactivation. */
export function activate(_context: vscode.ExtensionContext): void {
  const command = vscode.workspace
    .getConfiguration("bastion")
    .get<string>("serverPath", "bastion");

  const serverOptions: ServerOptions = {
    command,
    args: ["lsp"],
    transport: TransportKind.stdio,
  };
  const clientOptions: LanguageClientOptions = {
    documentSelector: [{ scheme: "file", language: "bcl" }],
  };

  client = new LanguageClient(
    "bastion",
    "TPT Bastion",
    serverOptions,
    clientOptions
  );
  client.start();
}

export async function deactivate(): Promise<void> {
  if (client) {
    await client.stop();
    client = undefined;
  }
}
