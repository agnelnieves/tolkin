import Script from "next/script";

const GA_ID = "G-8QPMMZHYKZ";

// GA4, loaded after hydration. Rendered from the root layout only when
// NODE_ENV is production, so dev and CI builds stay network-silent.
export function Analytics() {
  return (
    <>
      <Script
        src={`https://www.googletagmanager.com/gtag/js?id=${GA_ID}`}
        strategy="afterInteractive"
      />
      <Script id="ga4-init" strategy="afterInteractive">
        {`
          window.dataLayer = window.dataLayer || [];
          function gtag(){dataLayer.push(arguments);}
          gtag('js', new Date());
          gtag('config', '${GA_ID}');
        `}
      </Script>
    </>
  );
}
