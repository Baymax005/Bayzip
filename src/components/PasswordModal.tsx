import { useModalDismiss } from "../hooks/useModalDismiss";

interface PasswordModalProps {
  isOpen: boolean;
  onClose: () => void;
  password: string;
  setPassword: (password: string) => void;
  showPassword: boolean;
  setShowPassword: (show: boolean) => void;
  isExtracting: boolean;
  extractError: string | null;
  onSubmit: () => void;
}

export function PasswordModal({
  isOpen,
  onClose,
  password,
  setPassword,
  showPassword,
  setShowPassword,
  isExtracting,
  extractError,
  onSubmit,
}: PasswordModalProps) {
  const { handleBackdropClick } = useModalDismiss({
    isOpen,
    onClose,
    disabled: isExtracting,
  });

  if (!isOpen) return null;

  return (
    <div
      onClick={handleBackdropClick}
      className="fixed inset-0 z-50 bg-black/70 backdrop-blur-sm flex items-center justify-center p-4 select-none animate-in fade-in duration-150"
    >
      <div className="bg-[#252525] border border-[#3E3E3E] rounded-xl w-full max-w-md shadow-2xl p-6 space-y-4 animate-in zoom-in-95 duration-150">
        <div className="flex items-center space-x-3">
          <div className="w-10 h-10 rounded-full bg-[#60CDFF]/15 border border-[#60CDFF]/30 flex items-center justify-center shrink-0">
            <svg className="w-5 h-5 text-[#60CDFF]" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 15v2m-6 4h12a2 2 0 002-2v-6a2 2 0 00-2-2H6a2 2 0 00-2 2v6a2 2 0 002 2zm10-10V7a4 4 0 00-8 0v4h8z" />
            </svg>
          </div>
          <div>
            <h3 className="text-sm font-semibold text-white">Password Required</h3>
            <p className="text-xs text-[#A8A8A8]">This archive is encrypted with AES. Enter the password to unlock.</p>
          </div>
        </div>

        <div>
          <label className="block text-xs font-medium text-[#C8C8C8] mb-1.5">
            Decryption Password
          </label>
          <div className="relative">
            <input
              type={showPassword ? "text" : "password"}
              value={password}
              onChange={(e) => setPassword(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter" && password.trim() && !isExtracting) {
                  e.preventDefault();
                  onSubmit();
                }
              }}
              autoFocus
              placeholder="Enter archive password..."
              className="w-full bg-[#1B1B1B] text-white text-sm pl-3 pr-10 py-2.5 rounded-lg border border-[#444] focus:border-[#60CDFF] outline-none font-mono placeholder-[#666]"
            />
            <button
              type="button"
              onClick={() => setShowPassword(!showPassword)}
              className="absolute inset-y-0 right-0 pr-3 flex items-center text-[#888] hover:text-white cursor-pointer"
              title={showPassword ? "Hide password" : "Show password"}
            >
              {showPassword ? (
                <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M13.875 18.825A10.05 10.05 0 0112 19c-4.478 0-8.268-2.943-9.543-7a9.97 9.97 0 011.563-3.029m5.858.908a3 3 0 114.243 4.243M9.878 9.878l4.242 4.242M9.88 9.88l-3.29-3.29m7.532 7.532l3.29 3.29M3 3l18 18" />
                </svg>
              ) : (
                <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M2.458 12C3.732 7.943 7.523 5 12 5c4.478 0 8.268 2.943 9.542 7-1.274 4.057-5.064 7-9.542 7-4.477 0-8.268-2.943-9.542-7z" />
                </svg>
              )}
            </button>
          </div>
        </div>

        {extractError && (
          <div className="text-xs text-rose-400 bg-rose-950/40 p-2.5 rounded border border-rose-900/50">
            {extractError}
          </div>
        )}

        <div className="flex justify-end space-x-2 pt-2">
          <button
            type="button"
            disabled={isExtracting}
            onClick={onClose}
            className="px-4 py-2 bg-[#333] hover:bg-[#3D3D3D] text-[#CCCCCC] text-xs font-medium rounded-lg transition-colors cursor-pointer disabled:opacity-50"
          >
            Cancel
          </button>
          <button
            type="button"
            disabled={!password.trim() || isExtracting}
            onClick={onSubmit}
            className="px-4 py-2 bg-[#60CDFF] hover:bg-[#52B2DE] disabled:opacity-50 disabled:cursor-not-allowed text-black text-xs font-semibold rounded-lg transition-colors cursor-pointer flex items-center space-x-1.5"
          >
            {isExtracting ? (
              <>
                <svg className="animate-spin h-3.5 w-3.5 text-black" fill="none" viewBox="0 0 24 24">
                  <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4"></circle>
                  <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
                </svg>
                <span>Decrypting...</span>
              </>
            ) : (
              <span>Decrypt & Extract</span>
            )}
          </button>
        </div>
      </div>
    </div>
  );
}
