/** @type {import('next').NextConfig} */
const nextConfig = {
  output: 'standalone',
  serverExternalPackages: ['@kubernetes/client-node'],
};

export default nextConfig;
