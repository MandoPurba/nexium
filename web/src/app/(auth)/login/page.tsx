"use client";

import { useState } from "react";
import Link from "next/link";
import { useRouter } from "next/navigation";
import { useAuth } from "@/lib/auth-context";
import { auth, ApiError } from "@/lib/api";

export default function LoginPage() {
  const router = useRouter();
  const { login } = useAuth();
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [error, setError] = useState("");
  const [loading, setLoading] = useState(false);

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    setError("");
    setLoading(true);
    try {
      const res = await auth.login(email, password);
      login(res.access_token);
      router.push("/trade");
    } catch (err) {
      setError((err as ApiError).message || "Login failed");
    } finally {
      setLoading(false);
    }
  }

  return (
    <div className="flex flex-1 min-h-0">
      {/* Brand panel */}
      <div className="hidden lg:flex flex-1 flex-col justify-between p-12 border-r border-[#1c2127] relative overflow-hidden"
        style={{ background: "radial-gradient(120% 100% at 0% 0%, #11271d 0%, #0b0e11 55%)" }}>
        <div className="flex items-center gap-3 relative z-10">
          <div className="w-[30px] h-[30px] bg-gradient-to-br from-[#2ebd85] to-[#1a8f63] rotate-45 rounded-[7px] flex items-center justify-center">
            <div className="w-[10px] h-[10px] bg-[#0b0e11] rotate-45 rounded-[1px]" />
          </div>
          <span className="text-xl font-bold">NEXIUM</span>
        </div>
        <div className="relative z-10">
          <h2 className="text-[40px] font-bold leading-[1.1] tracking-[-0.03em] mb-5 max-w-[460px]">
            The exchange built for serious traders.
          </h2>
          <p className="text-[15px] text-[#9aa4b0] leading-relaxed max-w-[420px]">
            Deep liquidity, sub-millisecond matching, and institutional-grade security. Trade 200+ spot pairs with fees as low as 0.10%.
          </p>
          <div className="flex gap-10 mt-9">
            <div>
              <div className="text-2xl font-bold font-mono">$48B+</div>
              <div className="text-xs text-[#5e6673]">24h volume</div>
            </div>
            <div>
              <div className="text-2xl font-bold font-mono">200+</div>
              <div className="text-xs text-[#5e6673]">Trading pairs</div>
            </div>
            <div>
              <div className="text-2xl font-bold font-mono">12M</div>
              <div className="text-xs text-[#5e6673]">Users</div>
            </div>
          </div>
        </div>
        <div className="text-xs text-[#3a4148] relative z-10">© 2026 Nexium Labs · Regulated &amp; Insured</div>
        <div className="absolute -right-[120px] -bottom-[120px] w-[420px] h-[420px] rounded-full" style={{ background: "radial-gradient(circle, rgba(46,189,133,0.12), transparent 70%)" }} />
      </div>

      {/* Form panel */}
      <div className="w-full lg:w-[480px] shrink-0 flex items-center justify-center p-12">
        <div className="w-full max-w-[360px]">
          <h1 className="text-[26px] font-bold tracking-[-0.02em] mb-1.5">Welcome back</h1>
          <p className="text-sm text-[#848e9c] mb-7">Sign in to your Nexium account.</p>

          {/* Tabs */}
          <div className="flex bg-[#11151a] rounded-[9px] p-[3px] mb-6">
            <div className="flex-1 text-center py-2.5 text-[13px] font-semibold rounded-[7px] bg-[#1a1f25] text-foreground cursor-pointer">
              Sign in
            </div>
            <Link href="/register" className="flex-1 text-center py-2.5 text-[13px] font-semibold rounded-[7px] text-[#5e6673] cursor-pointer hover:text-[#848e9c] transition-colors">
              Create account
            </Link>
          </div>

          <form onSubmit={handleSubmit} className="flex flex-col gap-4">
            {error && (
              <div className="rounded-lg bg-[#f6465d]/10 px-3 py-2.5 text-sm text-[#f6465d]">{error}</div>
            )}
            <div>
              <div className="text-xs text-[#848e9c] mb-1.5">Email</div>
              <div className="flex items-center bg-[#11151a] border border-[#242a31] rounded-[9px] px-3.5 h-[46px] focus-within:border-[#2ebd85]/60 transition-colors">
                <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="#5e6673" strokeWidth="2"><rect x="3" y="5" width="18" height="14" rx="2" /><path d="m3 7 9 6 9-6" /></svg>
                <input
                  type="email"
                  placeholder="you@example.com"
                  value={email}
                  onChange={(e) => setEmail(e.target.value)}
                  required
                  className="flex-1 bg-transparent border-none text-sm text-[#eaecef] ml-2.5 focus:outline-none placeholder:text-[#5e6673]"
                />
              </div>
            </div>
            <div>
              <div className="flex justify-between mb-1.5">
                <span className="text-xs text-[#848e9c]">Password</span>
                <span className="text-xs text-[#2ebd85] cursor-pointer">Forgot?</span>
              </div>
              <div className="flex items-center bg-[#11151a] border border-[#242a31] rounded-[9px] px-3.5 h-[46px] focus-within:border-[#2ebd85]/60 transition-colors">
                <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="#5e6673" strokeWidth="2"><rect x="4" y="11" width="16" height="9" rx="2" /><path d="M8 11V7a4 4 0 0 1 8 0v4" /></svg>
                <input
                  type="password"
                  value={password}
                  onChange={(e) => setPassword(e.target.value)}
                  required
                  className="flex-1 bg-transparent border-none text-sm text-[#eaecef] ml-2.5 focus:outline-none placeholder:text-[#5e6673]"
                />
              </div>
            </div>
            <button
              type="submit"
              disabled={loading}
              className="py-3.5 text-sm font-semibold text-center bg-[#2ebd85] text-[#04130c] rounded-[9px] cursor-pointer hover:bg-[#26a87a] transition-colors disabled:opacity-60"
            >
              {loading ? "Signing in…" : "Sign in"}
            </button>
            <div className="flex items-center gap-3 text-xs text-[#3a4148]">
              <span className="flex-1 h-px bg-[#1c2127]" />
              or
              <span className="flex-1 h-px bg-[#1c2127]" />
            </div>
            <div className="flex gap-2.5">
              <button type="button" className="flex-1 flex items-center justify-center gap-2 py-2.5 text-[13px] font-medium bg-[#11151a] border border-[#242a31] rounded-[9px] text-[#cdd2d8] cursor-pointer hover:border-[#2a313a] transition-colors">
                Google
              </button>
              <button type="button" className="flex-1 flex items-center justify-center gap-2 py-2.5 text-[13px] font-medium bg-[#11151a] border border-[#242a31] rounded-[9px] text-[#cdd2d8] cursor-pointer hover:border-[#2a313a] transition-colors">
                Passkey
              </button>
            </div>
          </form>
          <p className="text-center text-[13px] text-[#5e6673] mt-6">
            <Link href="/trade" className="hover:text-[#848e9c] transition-colors">← Continue as guest</Link>
          </p>
        </div>
      </div>
    </div>
  );
}
