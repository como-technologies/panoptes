'use client';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { useState, useEffect, createContext, useContext } from 'react';
import { ToastProvider } from '@/components/ui/toast';
import { useResourceStream } from '@/hooks/useResourceStream';
import { useEventStream } from '@/hooks/useEventStream';

type Theme = 'light' | 'dark' | 'system';

interface ThemeContextValue {
  theme: Theme;
  setTheme: (theme: Theme) => void;
}

const ThemeContext = createContext<ThemeContextValue | null>(null);

export function useTheme() {
  const context = useContext(ThemeContext);
  if (!context) {
    throw new Error('useTheme must be used within a ThemeProvider');
  }
  return context;
}

function ThemeProvider({ children }: { children: React.ReactNode }) {
  const [theme, setThemeState] = useState<Theme>('system');
  const [mounted, setMounted] = useState(false);

  useEffect(() => {
    setMounted(true);
    const savedTheme = localStorage.getItem('panoptes.theme') as Theme | null;
    if (savedTheme) {
      setThemeState(savedTheme);
    }
  }, []);

  useEffect(() => {
    if (!mounted) return;

    const root = document.documentElement;

    if (theme === 'dark') {
      root.classList.add('dark');
    } else if (theme === 'light') {
      root.classList.remove('dark');
    } else {
      // System preference
      if (window.matchMedia('(prefers-color-scheme: dark)').matches) {
        root.classList.add('dark');
      } else {
        root.classList.remove('dark');
      }
    }
  }, [theme, mounted]);

  const setTheme = (newTheme: Theme) => {
    setThemeState(newTheme);
    localStorage.setItem('panoptes.theme', newTheme);
  };

  return (
    <ThemeContext.Provider value={{ theme, setTheme }}>
      {children}
    </ThemeContext.Provider>
  );
}

/**
 * Component that enables the K8s resource stream for real-time updates.
 * Subscribes to SSE endpoint and invalidates React Query caches on changes.
 */
function ResourceStreamSubscriber({ children }: { children: React.ReactNode }) {
  useResourceStream({
    enabled: true,
    onConnectionChange: (connected) => {
      if (connected) {
        console.log('Real-time resource updates enabled');
      }
    },
  });

  return <>{children}</>;
}

/**
 * Component that listens to the global event stream and updates the Zustand store.
 * This runs in the background to track event counts and alerts across all pages.
 * Note: Uses getState() internally to avoid Zustand hooks during render (SSR-safe).
 */
function EventStreamSubscriber({ children }: { children: React.ReactNode }) {
  useEventStream({
    enabled: true,
    onConnect: () => {
      console.log('Global event stream connected - tracking events');
    },
    onDisconnect: () => {
      console.log('Global event stream disconnected');
    },
  });

  return <>{children}</>;
}

export function Providers({ children }: { children: React.ReactNode }) {
  const [queryClient] = useState(
    () =>
      new QueryClient({
        defaultOptions: {
          queries: {
            // Data is considered fresh for 60 seconds
            staleTime: 60 * 1000,
            // No automatic polling - updates come via SSE stream
            // Fallback refetch for metrics which don't have K8s watch
            refetchInterval: false,
          },
        },
      })
  );

  return (
    <QueryClientProvider client={queryClient}>
      <ResourceStreamSubscriber>
        <EventStreamSubscriber>
          <ThemeProvider>
            <ToastProvider>
              {children}
            </ToastProvider>
          </ThemeProvider>
        </EventStreamSubscriber>
      </ResourceStreamSubscriber>
    </QueryClientProvider>
  );
}
