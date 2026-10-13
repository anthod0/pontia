<script lang="ts">
  import LegalPage from "$lib/components/LegalPage.svelte";
</script>

<LegalPage
  title="Privacy Policy"
  description="How Pontia collects, uses, and protects information when you use its website and hosted services."
>
  <h1>Privacy Policy</h1>
  <p class="effective-date">Effective October 13, 2026</p>

  <p>
    This Privacy Policy explains how Pontia ("Pontia," "we," "us," or "our") handles information
    when you use pontia.dev, the hosted Pontia Cloud service, and the hosted Remote Dashboard
    (collectively, the "Hosted Services").
  </p>
  <p>
    Pontia is local-first software. Coding agents run on your device, and remote Dashboard traffic
    is encrypted end to end between your browser and your device. Pontia Cloud coordinates identity,
    device registration, and remote authorization; it does not store your project files or the
    plaintext contents of your agent sessions.
  </p>

  <h2>1. Scope</h2>
  <p>
    This policy covers only the Hosted Services operated at pontia.dev. It does not govern
    third-party services, coding agents, model providers, or Pontia infrastructure you or another
    person operate. Those services process information under their own policies. Data kept by Pontia
    on your device is controlled by you and is not sent to Pontia Cloud unless a feature explicitly
    requires it.
  </p>
  <p>
    Contributing code or documentation to Pontia does not make a contributor an operator of the
    Hosted Services or responsible for information processed by them.
  </p>

  <h2>2. Information we collect</h2>
  <h3>Account information</h3>
  <p>
    When you sign in with Google or GitHub, we receive your provider account identifier and may
    receive your display name, profile image, email address, and whether the provider has verified
    that address. We also keep records of linked sign-in methods and account creation and update
    times. We do not receive your Google or GitHub password.
  </p>

  <h3>Device and Edge information</h3>
  <p>
    If you enable remote access or deploy an Edge, we process identifiers, names, device handles,
    public encryption keys and key versions, ownership records, Edge hostnames and tunnel addresses,
    network addresses used to configure an Edge, and registration or authorization records. Secret
    credentials are stored in hashed form where the service only needs to verify them.
  </p>

  <h3>Authentication and security information</h3>
  <p>
    We process browser and CLI session records, short-lived authorization requests, expiration and
    revocation information, and rate-limit records. Pontia uses necessary cookies to complete OAuth
    sign-in, keep you signed in, and protect account flows. These cookies are not used for advertising.
  </p>

  <h3>Service and diagnostic information</h3>
  <p>
    Our infrastructure may process IP addresses, request headers, request timing, routes accessed,
    browser or device information, error details, and security events. Operational logs for Edge
    deployment can include an Edge identifier, hostname, and candidate network address. We use this
    information to operate, secure, troubleshoot, and prevent abuse of the Hosted Services.
  </p>

  <h3>Encrypted remote traffic</h3>
  <p>
    An Edge can observe routing information and the size and timing of encrypted traffic. It cannot
    decrypt Dashboard requests or responses. Pontia Cloud authorizes the connection but is not in the
    Dashboard business-traffic path. Your browser and device can access the plaintext needed to
    provide the Dashboard experience.
  </p>

  <h2>3. How we use information</h2>
  <p>We use information to:</p>
  <ul>
    <li>create and authenticate accounts and maintain sessions;</li>
    <li>register devices and Edges and authorize remote connections;</li>
    <li>provide, maintain, secure, and troubleshoot the Hosted Services;</li>
    <li>detect abuse, enforce rate limits, and protect users and infrastructure;</li>
    <li>communicate about service, security, or policy changes; and</li>
    <li>comply with applicable law and enforce our Terms of Service.</li>
  </ul>
  <h2>4. How we share information</h2>
  <p>We may share information in these limited circumstances:</p>
  <ul>
    <li>
      <strong>Infrastructure providers.</strong> The Hosted Services run on Cloudflare infrastructure,
      including Workers, D1, DNS, and observability services. Cloudflare processes service data on our
      behalf and under its own applicable terms.
    </li>
    <li>
      <strong>Sign-in providers.</strong> Google or GitHub receives information needed to complete the
      sign-in flow when you choose that provider.
    </li>
    <li>
      <strong>Legal and safety reasons.</strong> We may disclose information if reasonably necessary to
      comply with law, protect rights or safety, investigate abuse, or defend legal claims.
    </li>
  </ul>
  <p>We do not sell personal information or use it for cross-context behavioral advertising.</p>

  <h2>5. Cookies and analytics</h2>
  <p>
    Pontia uses only cookies needed for authentication, account linking, security, and session
    continuity. We do not currently use advertising cookies or a third-party product analytics service.
    Your browser may let you block cookies, but doing so can prevent sign-in and account features from
    working.
  </p>

  <h2>6. Retention</h2>
  <p>
    We retain account, registered device, and Edge records while they are needed to provide the Hosted
    Services. Authentication sessions and temporary authorization records have defined expiration
    periods, although records may remain for a limited time for security, cleanup, backup, or legal
    purposes. Operational logs and rate-limit data are kept only as long as reasonably necessary for
    security and service operation. We may retain information longer when required by law or needed to
    resolve disputes and enforce agreements.
  </p>

  <h2>7. Your choices and rights</h2>
  <p>
    Account settings let you review linked sign-in methods and revoke sessions. Device and Edge
    settings let you remove registrations you no longer use. Depending on where you live, you may also
    have rights to access, correct, delete, restrict, or receive a copy of personal information, or to
    object to certain processing.
  </p>
  <p>
    To make a privacy request, contact us as described below. We may need to verify that you control
    the relevant account. Removing Pontia Cloud records does not delete data held on your device, by an
    Agent Client, or by a third-party provider.
  </p>

  <h2>8. Security</h2>
  <p>
    We use technical and organizational safeguards designed to protect information, including HTTPS,
    restricted cookies, hashed credentials, scoped authorization, and end-to-end encryption for remote
    Dashboard traffic. No method of storage or transmission is completely secure, so we cannot
    guarantee absolute security. See the <a href="/docs/security">Security documentation</a> for the
    current trust boundaries and credential guidance.
  </p>

  <h2>9. Children</h2>
  <p>
    The Hosted Services are not directed to children under 13, and we do not knowingly collect their
    personal information. If you believe a child has provided personal information, contact us so we
    can take appropriate action.
  </p>

  <h2>10. Changes to this policy</h2>
  <p>
    We may update this policy as Pontia or applicable requirements change. We will post the updated
    policy here and change the effective date. If a change materially affects how we handle personal
    information, we will provide additional notice when reasonably possible.
  </p>

  <h2>11. Contact</h2>
  <p>
    For privacy questions or requests, contact Pontia at
    <a href="mailto:contact@pontia.dev">contact@pontia.dev</a>.
  </p>
</LegalPage>
