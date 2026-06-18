"use client";

import Link from "next/link";
import { useAuth } from "@/lib/auth-context";
import { usePathname } from "next/navigation";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";

export function Header() {
  const { user, logout } = useAuth();
  const pathname = usePathname();

  const navItems = [
    { label: "Markets", href: "/" },
    { label: "Trade", href: "/trade" },
    { label: "Wallet", href: "/wallet" },
  ];

  return (
    <header className="h-[60px] flex-none flex items-center gap-8 px-6 border-b border-border bg-[#0d1014] z-50">
      {/* Logo */}
      <Link href="/" className="flex items-center gap-2.5 shrink-0">
        <div className="w-[26px] h-[26px] bg-gradient-to-br from-[#2ebd85] to-[#1a8f63] rotate-45 rounded-[6px] flex items-center justify-center">
          <div className="w-[9px] h-[9px] bg-[#0d1014] rotate-45 rounded-[1px]" />
        </div>
        <span className="text-[18px] font-bold tracking-[-0.02em] text-foreground">
          NEXIUM
        </span>
      </Link>

      {/* Nav links */}
      <nav className="flex items-center gap-1">
        {navItems.map((item) => {
          const isActive = pathname === item.href || (item.href !== "/" && pathname.startsWith(item.href));
          return (
            <Link
              key={item.href}
              href={item.href}
              className={`px-3.5 py-2 text-sm font-medium rounded-[7px] transition-colors ${
                isActive
                  ? "bg-[#161a1e] text-foreground"
                  : "text-[#848e9c] hover:text-foreground"
              }`}
            >
              {item.label}
            </Link>
          );
        })}
        <span className="px-3.5 py-2 text-sm font-medium text-[#848e9c] rounded-[7px] cursor-not-allowed opacity-60">
          Earn
        </span>
      </nav>

      {/* Spacer */}
      <div className="flex-1" />

      {/* Search bar */}
      <div className="hidden md:flex items-center gap-2 h-9 px-3 bg-[#161a1e] border border-[#242a31] rounded-lg w-[220px]">
        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="#5e6673" strokeWidth="2">
          <circle cx="11" cy="11" r="7" />
          <path d="m20 20-3.5-3.5" />
        </svg>
        <span className="text-[13px] text-[#5e6673]">Search markets</span>
      </div>

      {/* Balance chip — only when logged in */}
      {user && (
        <div className="flex items-center gap-1.5 py-[7px] px-3 bg-[#11151a] border border-[#1f252c] rounded-lg">
          <span className="text-[12px] text-[#848e9c]">Balance</span>
          <span className="text-[13px] font-semibold font-mono text-foreground">$0.00</span>
        </div>
      )}

      {/* Auth */}
      <div className="flex items-center gap-2 shrink-0">
        {user ? (
          <DropdownMenu>
            <DropdownMenuTrigger className="flex items-center gap-2 h-9 px-3 bg-[#11151a] border border-[#1f252c] rounded-lg hover:border-[#2a313a] transition-colors">
                <div className="w-[34px] h-[34px] rounded-full bg-gradient-to-br from-[#3a4350] to-[#1c2127] flex items-center justify-center text-xs font-semibold text-[#aeb6c0]">
                  {user.email.charAt(0).toUpperCase()}
                </div>
                <span className="text-[13px] text-[#aeb6c0] max-w-[120px] truncate">
                  {user.email}
                </span>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end" className="bg-[#11151a] border-[#242a31]">
              <DropdownMenuItem
                onClick={logout}
                className="text-[#848e9c] hover:text-foreground cursor-pointer"
              >
                Sign out
              </DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>
        ) : (
          <>
            <Link href="/login">
              <button className="h-9 px-4 text-[13px] font-medium text-[#aeb6c0] bg-[#11151a] border border-[#242a31] rounded-lg hover:border-[#2a313a] hover:text-foreground transition-colors">
                Sign in
              </button>
            </Link>
            <Link href="/register">
              <button className="h-9 px-4 text-[13px] font-semibold bg-[#2ebd85] text-[#04130c] rounded-lg hover:bg-[#26a87a] transition-colors">
                Register
              </button>
            </Link>
          </>
        )}
      </div>
    </header>
  );
}
