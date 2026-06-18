export default function AuthLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  return (
    <div className="flex flex-col h-screen bg-[#0b0e11] overflow-hidden">
      {children}
    </div>
  );
}
