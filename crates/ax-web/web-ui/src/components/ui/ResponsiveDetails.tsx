import type {ReactNode} from 'react';
import useMobile from '../../hooks/useMobile';
import MobileControls from './MobileControls';

export default function ResponsiveDetails({title,children}: {title:string; children:ReactNode}) {
  const mobile=useMobile();
  return mobile ? <MobileControls className="review-inspect-content" title={title} icon="eye">{children}</MobileControls>
    : <details><summary>{title}</summary>{children}</details>;
}
