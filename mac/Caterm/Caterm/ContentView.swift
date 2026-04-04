//
//  ContentView.swift
//  Caterm
//
//  Created by eric on 2026/4/4.
//

import SwiftUI

#if os(macOS) && canImport(GhosttyKit)
import GhosttyKit
#endif

struct ContentView: View {
    private var ghosttyStatus: String {
        #if os(macOS) && canImport(GhosttyKit)
        "GhosttyKit linked"
        #elseif os(macOS)
        "GhosttyKit not linked"
        #else
        "Terminal runtime unavailable on this platform"
        #endif
    }

    var body: some View {
        VStack {
            Image(systemName: "globe")
                .imageScale(.large)
                .foregroundStyle(.tint)
            Text("Caterm")
            Text(ghosttyStatus)
                .font(.caption)
                .foregroundStyle(.secondary)
        }
        .padding()
    }
}

#Preview {
    ContentView()
}
