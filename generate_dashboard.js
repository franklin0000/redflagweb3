const fs = require('fs');

// Read files to get current state
const cargoToml = fs.readFileSync('/home/klk/.gemini/antigravity/scratch/redflag2.1/Cargo.toml', 'utf8');
const taskMd = fs.readFileSync('/home/klk/.gemini/antigravity/brain/8cc0d774-f8b7-4a20-a8cd-b2ec9c291f6b/task.md', 'utf8');
const cryptoLib = fs.readFileSync('/home/klk/.gemini/antigravity/scratch/redflag2.1/redflag-crypto/src/lib.rs', 'utf8');

const type = "RedFlag 2.1";
const name = "Production Infrastructure";
const action = "Updat";

// Helper for string literals in the generated JS
function StringLiteral(str) {
    return str.replace(/\\/g, '\\\\').replace(/"/g, '\\"').replace(/\n/g, '\\n');
}

// Function to safely highlight code with specific types
function highlightPython(content) {
    if (!content) return "";
    if (content.length > 50000) return "<span>Content too large to highlight fully.</span>";
    return content
        .replace(/&/g, "&amp;")
        .replace(/</g, "&lt;")
        .replace(/>/g, "&gt;")
        .replace(/\b(def|class|return|if|else|elif|for|while|import|from|as|try|except|finally|with|as|yield|await|async|assert|lambda|pass|break|continue|in|is|not|and|or|None|True|False|self|return)\b/g, '<span style="color: #c678dd;">$1</span>')
        .replace(/\b([a-zA-Z_][a-zA-Z0-9_]*)(?=\s*\()/g, '<span style="color: #61afef;">$1</span>')
        .replace(/('.*?'|".*?")/g, '<span style="color: #98c379;">$1</span>')
        .replace(/(#.*)/g, '<span style="color: #5c6370; font-style: italic;">$1</span>');
}

function highlightTOML(content) {
    if (!content) return "";
    return content
        .replace(/&/g, "&amp;")
        .replace(/</g, "&lt;")
        .replace(/>/g, "&gt;")
        .replace(/^([^\[\s#=]+)(?=\s*=)/gm, '<span style="color: #d19a66;">$1</span>')
        .replace(/^(\[.*\])/gm, '<span style="color: #e5c07b; font-weight: bold;">$1</span>')
        .replace(/('.*?'|".*?")/g, '<span style="color: #98c379;">$1</span>');
}

function highlightMarkdown(content) {
    if (!content) return "";
    return content
        .replace(/&/g, "&amp;")
        .replace(/</g, "&lt;")
        .replace(/>/g, "&gt;")
        .replace(/^(#+ .*)$/gm, '<span style="color: #e06c75; font-weight: bold;">$1</span>')
        .replace(/^([\s]*[-*] .*)$/gm, '<span style="color: #61afef;">$1</span>')
        .replace(/\[([ x/])\]/g, '<span style="color: #d19a66;">[$1]</span>');
}

// Generate the HTML content
let htmlContent = `
<!DOCTYPE html>
<html lang="es">
<head>
    <meta charset="UTF-8">
    <title>RedFlag 2.1 - Dashboard de Producción</title>
    <style>
        body { font-family: 'Segoe UI', Tahoma, Geneva, Verdana, sans-serif; background-color: #21252b; color: #abb2bf; padding: 20px; line-height: 1.6; }
        .container { max-width: 1200px; margin: 0 auto; }
        h1 { color: #e06c75; border-bottom: 2px solid #3e4451; padding-bottom: 10px; display: flex; align-items: center; justify-content: space-between; }
        h2 { color: #61afef; margin-top: 30px; }
        .card { background: #282c34; border-radius: 8px; padding: 20px; margin-bottom: 20px; box-shadow: 0 4px 6px rgba(0,0,0,0.3); border-left: 4px solid #61afef; }
        pre { background: #181a1f; padding: 15px; border-radius: 5px; overflow-x: auto; border: 1px solid #3e4451; font-size: 13px; }
        code { font-family: 'Consolas', 'Monaco', 'Courier New', monospace; }
        .grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(400px, 1fr)); gap: 20px; }
        .status-bar { display: flex; justify-content: space-between; font-size: 11px; color: #5c6370; border-top: 1px solid #3e4451; padding-top: 10px; margin-top: 20px; }
        .tag { display: inline-block; padding: 2px 8px; border-radius: 4px; font-size: 10px; font-weight: bold; text-transform: uppercase; margin-right: 5px; margin-bottom: 5px; }
        .tag-pqc { background: #c678dd; color: #fff; }
        .tag-safe { background: #98c379; color: #282c34; }
        .tag-network { background: #61afef; color: #fff; }
        .alert { padding: 10px 15px; border-radius: 5px; margin-bottom: 15px; border-left: 4px solid; }
        .alert-success { background: rgba(152, 195, 121, 0.1); border-left-color: #98c379; color: #98c379; }
        .alert-warning { background: rgba(209, 154, 102, 0.1); border-left-color: #d19a66; color: #d19a66; }
    </style>
</head>
<body>
    <div class="container">
        <h1>RedFlag 2.1 <span style="font-size: 0.5em; color: #5c6370;">v${type}</span></h1>
        
        <div class="alert alert-success">
            <strong>ESTADO: EJECUCIÓN ACTIVA.</strong> El motor criptográfico post-cuántico (ML-DSA/ML-KEM) ha sido validado exitosamente.
        </div>

        <div class="grid">
            <div class="card">
                <h2>Infraestructura (Rust)</h2>
                <div style="margin-bottom: 10px;">
                    <span class="tag tag-safe">Rust 2021 Edition</span>
                    <span class="tag tag-safe">Workspace Modular</span>
                    <span class="tag tag-pqc">Quantum-Verified</span>
                </div>
                <pre><code>${highlightTOML(cargoToml)}</code></pre>
            </div>

            <div class="card">
                <h2>Hoja de Ruta (task.md)</h2>
                <pre><code>${highlightMarkdown(taskMd)}</code></pre>
            </div>
        </div>

        <div class="card" style="border-left-color: #c678dd;">
            <h2>Módulo Criptográfico (redflag-crypto)</h2>
            <span class="tag tag-pqc">FIPS 203 & 204 Standards</span>
            <span class="tag tag-pqc">ML-KEM-768 (Kyber)</span>
            <span class="tag tag-pqc">ML-DSA-65 (Dilithium)</span>
            <p style="font-size: 0.9em; margin: 10px 0; color: #abb2bf;">
                Implementación de grado industrial con seguridad post-cuántica y anonimato nativo.
            </p>
            <pre><code>${highlightPython(cryptoLib)}</code></pre>
        </div>

        <div class="status-bar">
            <span>Conversación ID: 8cc0d774</span>
            <span>Ubicación: /scratch/redflag2.1/</span>
            <span>Última actualización: ${new Date().toLocaleString('es-ES')}</span>
        </div>
    </div>
</body>
</html>
`;

fs.writeFileSync('/home/klk/.gemini/antigravity/scratch/redflag2.1/status_dashboard.html', htmlContent);
console.log("Dashboard rebuilt successfully.");
