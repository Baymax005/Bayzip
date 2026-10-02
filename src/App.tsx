import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import "./App.css";

interface SystemSpecs {
  total_ram_gb: number;
  free_ram_gb: number;
  cpu_cores: number;
}

const FORMAT_ALGORITHMS: Record<string, string[]> = {
  ".bay": ["Zstandard (Recommended)", "Brotli", "LZ4 (Fastest)"],
  ".bay_gamer": ["Zstd-Fast (Auto)"],
  ".7z": ["LZMA2", "LZMA", "BZip2", "PPMd"],
  ".zip": ["Deflate", "Deflate64", "BZip2", "LZMA"],
  ".tar": ["None (Store)"]
};

function App() {
  const [specs, setSpecs] = useState<SystemSpecs | null>(null);
  const [sliderValue, setSliderValue] = useState<number>(50); // 0, 25, 50, 75, 100
  const [isAdvanced, setIsAdvanced] = useState(false);
  
  const [selectedFormat, setSelectedFormat] = useState(".bay");
  const [selectedAlgorithm, setSelectedAlgorithm] = useState(FORMAT_ALGORITHMS[".bay"][0]);
  const [selectedEncryption, setSelectedEncryption] = useState("none");

  // New Toggles
  const [archiveToOriginalPath, setArchiveToOriginalPath] = useState(true);
  const [separateArchives, setSeparateArchives] = useState(false);

  useEffect(() => {
    invoke<SystemSpecs>("get_system_specs")
      .then((res) => setSpecs(res))
      .catch((err) => console.error(err));
  }, []);

  useEffect(() => {
    setSelectedAlgorithm(FORMAT_ALGORITHMS[selectedFormat][0]);
  }, [selectedFormat]);

  const getEstimatedRam = () => {
    if (sliderValue === 0) return "0.0"; 
    if (!specs) return "0.5";
    const maxSafeRam = Math.max(0.5, specs.free_ram_gb - 1.5);
    return ((sliderValue / 100) * maxSafeRam).toFixed(1);
  };

  const getEstimatedCores = () => {
    if (sliderValue === 0) return 1; 
    if (!specs) return 1;
    return Math.max(1, Math.round((sliderValue / 100) * specs.cpu_cores));
  };

  const handleCompress = () => {
    console.log(`Executing compression. Format: ${selectedFormat}, Algo: ${selectedAlgorithm}`);
  };

  const getSliderLabel = () => {
    if (sliderValue === 0) return "STORE (No Compression)";
    if (sliderValue === 25) return "FAST";
    if (sliderValue === 50) return "BALANCED";
    if (sliderValue === 75) return "HIGH COMPRESSION";
    return "MAX COMPRESSION";
  };

  return (
    <div className="min-h-screen bg-[#202020] text-white flex flex-col font-sans select-none overflow-hidden">
      {/* Windows 11 Native Titlebar */}
      <div 
        data-tauri-drag-region="true" 
        className="h-10 w-full flex justify-between items-center bg-[#202020] hover:bg-[#202020] transition-colors cursor-default"
      >
        <div className="flex items-center space-x-3 px-4 pointer-events-none">
          <div className="w-4 h-4 text-[#60CDFF] flex items-center justify-center">
            {/* Simple Zip Icon */}
            <svg viewBox="0 0 24 24" fill="currentColor" className="w-full h-full"><path d="M14 2H6c-1.1 0-1.99.9-1.99 2L4 20c0 1.1.89 2 1.99 2h12c1.1 0 2-.9 2-2V8l-6-6zm-2 14h-2v-2h2v2zm0-4h-2v-2h2v2zm0-4h-2V6h2v2z"/></svg>
          </div>
          <span className="text-xs font-normal text-white">Bayzip</span>
        </div>
        <div className="flex h-full">
          {/* Windows Minimize */}
          <button 
            onClick={() => {
              import('@tauri-apps/api/window').then((m) => m.getCurrentWindow().minimize());
            }}
            className="h-full px-4 hover:bg-[#3E3E3E] flex items-center justify-center text-[#A8A8A8] transition-colors">
            <svg width="10" height="1" viewBox="0 0 10 1"><rect width="10" height="1" fill="currentColor"/></svg>
          </button>
          {/* Windows Maximize */}
          <button 
            onClick={() => {
              import('@tauri-apps/api/window').then((m) => m.getCurrentWindow().toggleMaximize());
            }}
            className="h-full px-4 hover:bg-[#3E3E3E] flex items-center justify-center text-[#A8A8A8] transition-colors">
            <svg width="10" height="10" viewBox="0 0 10 10"><path d="M0,0v10h10V0H0z M9,9H1V1h8V9z" fill="currentColor"/></svg>
          </button>
          {/* Windows Close */}
          <button 
            onClick={() => {
              import('@tauri-apps/api/window').then((m) => m.getCurrentWindow().close());
            }}
            className="h-full px-4 hover:bg-red-600 hover:text-white flex items-center justify-center text-[#A8A8A8] transition-colors">
            <svg width="10" height="10" viewBox="0 0 10 10"><path d="M10,1L9,0L5,4L1,0L0,1l4,4l-4,4l1,1l4-4l4,4l1-1l-4-4L10,1z" fill="currentColor"/></svg>
          </button>
        </div>
      </div>

      <div className="flex-1 flex flex-col p-6 overflow-y-auto">
        <div className="w-full max-w-2xl mx-auto space-y-6">
          
          {/* Header */}
          <div className="flex items-center justify-between">
            <h1 className="text-2xl font-semibold tracking-tight">Create Archive</h1>
            <button 
              onClick={() => setIsAdvanced(!isAdvanced)}
              className="text-sm font-medium px-4 py-1.5 rounded bg-[#333333] hover:bg-[#3E3E3E] border border-[#3E3E3E] transition-colors"
            >
              {isAdvanced ? "Simple Mode" : "Advanced Options"}
            </button>
          </div>

          {/* Windows 11 File Drop Zone */}
          <div 
            onClick={async () => {
              const { open } = await import('@tauri-apps/plugin-dialog');
              const selected = await open({
                multiple: true,
                title: 'Select files to compress'
              });
              if (selected) {
                console.log('Selected files:', selected);
              }
            }}
            className="bg-[#2D2D2D] border border-[#3E3E3E] rounded-lg p-10 text-center hover:bg-[#333333] transition-colors cursor-pointer flex flex-col items-center justify-center space-y-3">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" className="w-10 h-10 text-[#60CDFF]"><path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M12 4v16m8-8H4" /></svg>
            <p className="text-sm font-medium text-white">Drag and drop files to compress</p>
            <p className="text-xs text-[#A8A8A8]">or right-click files in Explorer</p>
          </div>

          {/* Hardware Configuration - Fluent Design Card */}
          <div className="bg-[#2D2D2D] rounded-lg border border-[#3E3E3E] p-6">
            <div className="flex justify-between items-center mb-6">
              <h3 className="text-sm font-medium">Hardware Allocation</h3>
              <span className="text-xs bg-[#3E3E3E] text-[#A8A8A8] px-2 py-1 rounded-md">
                HARDWARE PROFILING ACTIVE
              </span>
            </div>

            <div className="mb-6">
              <div className="flex justify-between items-end mb-2">
                 <span className="text-sm font-semibold text-[#60CDFF]">{getSliderLabel()}</span>
              </div>
              
              <div className="relative pt-2 pb-8">
                <input 
                  type="range" 
                  min="0" 
                  max="100" 
                  step="25"
                  value={sliderValue}
                  onChange={(e) => setSliderValue(Number(e.target.value))}
                  className="w-full h-1 bg-[#3E3E3E] rounded-lg appearance-none cursor-pointer accent-[#60CDFF] relative z-10" 
                />
                <div className="absolute w-[calc(100%-16px)] left-[8px] mt-2">
                  <span className="absolute left-[0%] -translate-x-1/2 text-xs text-[#A8A8A8]">0</span>
                  <span className="absolute left-[25%] -translate-x-1/2 text-xs text-[#A8A8A8]">25</span>
                  <span className="absolute left-[50%] -translate-x-1/2 text-xs text-[#A8A8A8]">50</span>
                  <span className="absolute left-[75%] -translate-x-1/2 text-xs text-[#A8A8A8]">75</span>
                  <span className="absolute left-[100%] -translate-x-1/2 text-xs text-[#A8A8A8]">100</span>
                </div>
              </div>
            </div>

            {/* Metrics */}
            <div className="grid grid-cols-2 gap-4">
              <div className="bg-[#202020] p-4 rounded-md border border-[#3E3E3E]">
                <p className="text-xs text-[#A8A8A8] mb-1">Target RAM Usage</p>
                <p className="text-lg font-medium">{getEstimatedRam()} <span className="text-sm font-normal text-[#A8A8A8]">GB</span></p>
              </div>
              <div className="bg-[#202020] p-4 rounded-md border border-[#3E3E3E]">
                <p className="text-xs text-[#A8A8A8] mb-1">Target CPU Threads</p>
                <p className="text-lg font-medium">{getEstimatedCores()} <span className="text-sm font-normal text-[#A8A8A8]">/ {specs?.cpu_cores || '?'}</span></p>
              </div>
            </div>
          </div>

          {/* Advanced Options Toggle */}
          {isAdvanced && (
            <div className="bg-[#2D2D2D] rounded-lg border border-[#3E3E3E] p-6 space-y-5">
               <div className="grid grid-cols-3 gap-4">
                  <div className={selectedFormat === ".bay_gamer" ? "col-span-2" : "col-span-1"}>
                    <label className="block text-xs font-medium mb-2">Format</label>
                    <select 
                      value={selectedFormat}
                      onChange={(e) => setSelectedFormat(e.target.value)}
                      className="w-full bg-[#202020] rounded-md p-2 text-sm border border-[#3E3E3E] focus:border-[#60CDFF] outline-none"
                    >
                      <option value=".bay">.bay</option>
                      <option value=".bay_gamer">.bay (Gamer Mod Pack)</option>
                      <option value=".7z">.7z</option>
                      <option value=".zip">.zip</option>
                      <option value=".tar">.tar</option>
                    </select>
                  </div>
                  
                  {/* Hide algorithm dropdown entirely if Gamer Mod Pack is selected */}
                  {selectedFormat !== ".bay_gamer" && (
                    <div>
                      <label className="block text-xs font-medium mb-2">Algorithm</label>
                      <select 
                        value={selectedAlgorithm}
                        onChange={(e) => setSelectedAlgorithm(e.target.value)}
                        className="w-full bg-[#202020] rounded-md p-2 text-sm border border-[#3E3E3E] focus:border-[#60CDFF] outline-none"
                      >
                        {FORMAT_ALGORITHMS[selectedFormat].map((algo) => (
                          <option key={algo} value={algo}>{algo}</option>
                        ))}
                      </select>
                    </div>
                  )}

                  <div>
                    <label className="block text-xs font-medium mb-2">Encryption</label>
                    <select 
                      value={selectedEncryption}
                      onChange={(e) => setSelectedEncryption(e.target.value)}
                      className="w-full bg-[#202020] rounded-md p-2 text-sm border border-[#3E3E3E] focus:border-[#60CDFF] outline-none"
                    >
                      <option value="none">None</option>
                      <option value="aes-256">AES-256-GCM</option>
                      <option value="chacha20">ChaCha20-Poly1305</option>
                      <option value="twofish">Twofish</option>
                    </select>
                  </div>
               </div>
               
               {selectedEncryption !== "none" && (
                 <div>
                    <label className="block text-xs font-medium mb-2">Password</label>
                    <input type="password" placeholder="Enter secure password" className="w-full bg-[#202020] rounded-md p-2 text-sm border border-[#3E3E3E] focus:border-[#60CDFF] outline-none placeholder-[#A8A8A8]" />
                 </div>
               )}

               {/* Extra Options Toggles */}
               <div className="pt-2 border-t border-[#3E3E3E] flex flex-col space-y-4">
                 <label className="flex items-center cursor-pointer group">
                   <div className="relative">
                     <input 
                       type="checkbox" 
                       className="sr-only" 
                       checked={archiveToOriginalPath} 
                       onChange={() => setArchiveToOriginalPath(!archiveToOriginalPath)} 
                     />
                     <div className={`block w-10 h-6 rounded-full transition-colors ${archiveToOriginalPath ? 'bg-[#60CDFF]' : 'bg-[#3E3E3E]'}`}></div>
                     <div className={`dot absolute left-1 top-1 bg-white w-4 h-4 rounded-full transition-transform ${archiveToOriginalPath ? 'transform translate-x-4' : ''}`}></div>
                   </div>
                   <div className="ml-3 text-sm text-[#A8A8A8] group-hover:text-white transition-colors">
                     Archive to original path
                   </div>
                 </label>

                 <label className="flex items-center cursor-pointer group">
                   <div className="relative">
                     <input 
                       type="checkbox" 
                       className="sr-only" 
                       checked={separateArchives} 
                       onChange={() => setSeparateArchives(!separateArchives)} 
                     />
                     <div className={`block w-10 h-6 rounded-full transition-colors ${separateArchives ? 'bg-[#60CDFF]' : 'bg-[#3E3E3E]'}`}></div>
                     <div className={`dot absolute left-1 top-1 bg-white w-4 h-4 rounded-full transition-transform ${separateArchives ? 'transform translate-x-4' : ''}`}></div>
                   </div>
                   <div className="ml-3 text-sm text-[#A8A8A8] group-hover:text-white transition-colors">
                     Add to separate archives
                   </div>
                 </label>
               </div>
            </div>
          )}

          {/* Execute Button */}
          <div className="flex justify-end pt-2">
            <button 
              onClick={handleCompress}
              className="px-8 py-2 bg-[#60CDFF] hover:bg-[#4FB8E6] text-black font-semibold rounded-md shadow-sm transition-colors text-sm"
            >
              Compress
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}

export default App;
