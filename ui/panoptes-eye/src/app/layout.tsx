import type { Metadata } from 'next';
import { Inter, Ubuntu_Mono } from 'next/font/google';
import './globals.css';
import { Providers } from './providers';
import { Navigation } from '@/components/navigation';

const inter = Inter({ subsets: ['latin'], variable: '--font-inter' });
const ubuntuMono = Ubuntu_Mono({
  weight: ['400', '700'],
  subsets: ['latin'],
  variable: '--font-ubuntu-mono',
});

export const metadata: Metadata = {
  title: 'Panoptes Eye - Security Monitoring',
  description: 'All-seeing Kubernetes file integrity and access monitoring',
};

export default function RootLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  return (
    <html lang="en" suppressHydrationWarning>
      <body className={`${inter.variable} ${ubuntuMono.variable} font-sans`}>
        <Providers>
          <div className="flex h-screen">
            <Navigation />
            <main className="flex-1 overflow-auto p-6 bg-background">
              {children}
            </main>
          </div>
        </Providers>
      </body>
    </html>
  );
}
